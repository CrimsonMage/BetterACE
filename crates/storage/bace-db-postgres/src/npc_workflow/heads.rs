//! Per-source CAS is separate from invocation stage versions. It prevents two
//! overlapping continuations from publishing incompatible full VM snapshots.
use crate::{PgStore, StoreError};
use bace_persistence::NpcWorkflowUpdate;
use bace_storage_codec::NpcWorkflowSaveV3;
use sqlx::{PgConnection, Postgres, Row, Transaction};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredNpcSourceHead {
    pub source: u32,
    pub source_version: u64,
    pub source_template: u32,
    pub invocation: [u8; 16],
    pub workflow_version: i64,
    pub completed: bool,
    pub checkpoint: Vec<u8>,
}
fn row_head(row: sqlx::postgres::PgRow, source: u32) -> Result<StoredNpcSourceHead, StoreError> {
    let checkpoint: Vec<u8> = row.get("checkpoint");
    if checkpoint.len() > 4_194_356 {
        return Err(StoreError::Invalid("NPC source checkpoint bound"));
    }
    let value = NpcWorkflowSaveV3::decode_or_migrate(&checkpoint)
        .map_err(|_| StoreError::Invalid("NPC source checkpoint"))?;
    let source_version = u64::try_from(row.get::<i64, _>("source_version"))
        .map_err(|_| StoreError::Invalid("NPC source version"))?;
    let invocation: [u8; 16] = row
        .get::<Vec<u8>, _>("invocation")
        .try_into()
        .map_err(|_| StoreError::Invalid("NPC invocation"))?;
    let completed: bool = row.get("completed");
    let archived: bool = row.get("archived");
    let workflow_version: i64 = row.get("workflow_version");
    let cell: Option<i64> = row.get("source_cell");
    if value.location.as_ref().map(|v| i64::from(v.cell)) != cell {
        return Err(StoreError::Invalid("NPC source cell locator"));
    }
    if value.archive.is_some() != archived
        || value.stage.checked_add(1) != u64::try_from(workflow_version).ok()
        || value.source != source
        || value.invocation != invocation
        || value.completed != completed
        || (value.source_version != 0 && value.source_version != source_version)
    {
        return Err(StoreError::Invalid("NPC source head identity"));
    }
    Ok(StoredNpcSourceHead {
        source,
        source_version,
        source_template: value.source_template,
        invocation,
        workflow_version: row.get("workflow_version"),
        completed,
        checkpoint,
    })
}
async fn legacy(
    connection: &mut PgConnection,
    source: u32,
) -> Result<Option<StoredNpcSourceHead>, StoreError> {
    let mut rows=sqlx::query("SELECT invocation,version AS workflow_version,0::bigint AS source_version,completed,false AS archived,NULL::bigint AS source_cell,checkpoint FROM npc_workflows WHERE source_id=$1 ORDER BY invocation LIMIT 2").bind(i64::from(source)).fetch_all(&mut *connection).await?;
    if rows.len() > 1 {
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM npc_workflows WHERE source_id=$1 AND NOT completed)",
        )
        .bind(i64::from(source))
        .fetch_one(&mut *connection)
        .await?;
        if active {
            return Err(StoreError::Invalid("ambiguous legacy NPC source history"));
        }
        return Ok(None);
    }
    rows.pop().map(|row| row_head(row, source)).transpose()
}
impl PgStore {
    /// A stored source moved elsewhere (or destroyed) must not also appear at
    /// its original authored placement. This reads only one bounded region's IDs.
    pub async fn npc_sources_outside_region(
        &self,
        ids: &[u32],
        landblock: u16,
    ) -> Result<Vec<u32>, StoreError> {
        if ids.len() > 4096 || ids.contains(&0) {
            return Err(StoreError::Invalid("NPC regional source identity bounds"));
        }
        let ids: Vec<_> = ids.iter().map(|id| i64::from(*id)).collect();
        let rows=sqlx::query_scalar::<_,i64>("SELECT source_id FROM npc_source_heads WHERE source_id=ANY($1) AND (archived OR source_cell IS NOT NULL AND (source_cell >> 16)<>$2) ORDER BY source_id").bind(ids).bind(i64::from(landblock)).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|id| u32::try_from(id).map_err(|_| StoreError::Invalid("NPC source identity")))
            .collect()
    }
    /// One canonical source checkpoint, including a completed head's retained
    /// version. Multiple unresolved legacy histories fail instead of guessing.
    pub async fn npc_source_head(
        &self,
        source: u32,
    ) -> Result<Option<StoredNpcSourceHead>, StoreError> {
        if source == 0 {
            return Err(StoreError::Invalid("NPC source identity"));
        }
        let mut connection = self.pool.acquire().await?;
        let row=sqlx::query("SELECT invocation,workflow_version,version AS source_version,completed,archived,source_cell,checkpoint FROM npc_source_heads WHERE source_id=$1").bind(i64::from(source)).fetch_optional(&mut *connection).await?;
        if let Some(row) = row {
            return row_head(row, source).map(Some);
        }
        legacy(&mut connection, source).await
    }
    /// Keyset page of actual accepted live source locations, including completed
    /// heads whose qualities/quests survive ordinary region recreation.
    pub async fn npc_source_heads_in_region(
        &self,
        landblock: u16,
        after: Option<u32>,
        limit: u32,
    ) -> Result<Vec<StoredNpcSourceHead>, StoreError> {
        if !(1..=16).contains(&limit) {
            return Err(StoreError::Invalid("NPC regional source page"));
        }
        let rows=sqlx::query("SELECT source_id,invocation,workflow_version,version AS source_version,completed,archived,source_cell,checkpoint FROM npc_source_heads WHERE NOT archived AND (source_cell >> 16)=$1 AND source_id>$2 ORDER BY source_id LIMIT $3").bind(i64::from(landblock)).bind(i64::from(after.unwrap_or(0))).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                let source = u32::try_from(row.get::<i64, _>("source_id"))
                    .map_err(|_| StoreError::Invalid("NPC source identity"))?;
                row_head(row, source)
            })
            .collect()
    }
    /// A completed detached source is still destroyed. Keep static admission
    /// suppressed until an explicit later canonical head recreates that source.
    pub async fn archived_npc_source_ids(
        &self,
        after: Option<u32>,
        limit: u32,
    ) -> Result<Vec<u32>, StoreError> {
        if !(1..=256).contains(&limit) {
            return Err(StoreError::Invalid("NPC archive page"));
        }
        let rows=sqlx::query_scalar::<_,i64>("SELECT source_id FROM npc_source_heads WHERE archived AND source_id>$1 ORDER BY source_id LIMIT $2").bind(i64::from(after.unwrap_or(0))).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|id| u32::try_from(id).map_err(|_| StoreError::Invalid("NPC source identity")))
            .collect()
    }
    pub async fn suppressed_npc_source_ids(
        &self,
        after: Option<u32>,
        limit: u32,
    ) -> Result<Vec<u32>, StoreError> {
        if !(1..=256).contains(&limit) {
            return Err(StoreError::Invalid("NPC source page"));
        }
        let rows=sqlx::query_scalar::<_,i64>("SELECT source_id FROM (SELECT source_id FROM npc_source_heads WHERE (NOT completed OR archived) AND source_id>$1 UNION SELECT source_id FROM npc_workflows w WHERE NOT completed AND source_id>$1 AND NOT EXISTS(SELECT 1 FROM npc_source_heads h WHERE h.source_id=w.source_id)) sources ORDER BY source_id LIMIT $2").bind(i64::from(after.unwrap_or(0))).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|id| u32::try_from(id).map_err(|_| StoreError::Invalid("NPC source identity")))
            .collect()
    }
    pub async fn pending_npc_source_ids(
        &self,
        after: Option<u32>,
        limit: u32,
    ) -> Result<Vec<u32>, StoreError> {
        if !(1..=256).contains(&limit) {
            return Err(StoreError::Invalid("NPC source page"));
        }
        let rows=sqlx::query_scalar::<_,i64>("SELECT source_id FROM (SELECT source_id FROM npc_source_heads WHERE NOT completed AND source_id>$1 UNION SELECT source_id FROM npc_workflows w WHERE NOT completed AND source_id>$1 AND NOT EXISTS(SELECT 1 FROM npc_source_heads h WHERE h.source_id=w.source_id)) sources ORDER BY source_id LIMIT $2").bind(i64::from(after.unwrap_or(0))).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|id| u32::try_from(id).map_err(|_| StoreError::Invalid("NPC source identity")))
            .collect()
    }
}
pub(super) async fn prepare(
    tx: &mut Transaction<'_, Postgres>,
    update: &NpcWorkflowUpdate,
    value: &NpcWorkflowSaveV3,
) -> Result<u64, StoreError> {
    let source = value.source;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind((0x4e50_i64 << 32) | i64::from(source))
        .execute(&mut **tx)
        .await?;
    let row=sqlx::query("SELECT invocation,workflow_version,version AS source_version,completed,archived,source_cell,checkpoint FROM npc_source_heads WHERE source_id=$1 FOR UPDATE").bind(i64::from(source)).fetch_optional(&mut **tx).await?;
    let head = if let Some(row) = row {
        Some(row_head(row, source)?)
    } else {
        legacy(&mut *tx, source).await?
    };
    let version = head
        .as_ref()
        .map_or(0, |h| h.source_version)
        .checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or(StoreError::Invalid("NPC source version overflow"))?;
    if value.source_version != 0 && value.source_version != version {
        return Err(StoreError::Conflict(source));
    }
    if let Some(head) = head {
        let old = NpcWorkflowSaveV3::decode_or_migrate(&head.checkpoint)
            .map_err(|_| StoreError::Invalid("prior NPC source head"))?;
        if value.source_version == 0
            && (old.source_version != 0 || head.invocation != update.invocation && !head.completed)
        {
            return Err(StoreError::Invalid(
                "legacy NPC source writer cannot replace canonical head",
            ));
        }
        if !old.completed {
            if old.source_template != 0 && old.source_template != value.source_template {
                return Err(StoreError::Invalid("NPC active source template change"));
            }
            if old.program_hash != value.program_hash
                || old.content_generation != value.content_generation
                || old.key_version != value.key_version
                || old.logical_now > value.logical_now
                || old.next_order > value.next_order
                || old.remaining_instructions < value.remaining_instructions
                || old.active_operation > value.active_operation
            {
                return Err(StoreError::Invalid(
                    "NPC source head rewind or definition change",
                ));
            }
            for old in &old.invocations {
                if let Some(new) = value
                    .invocations
                    .iter()
                    .find(|i| i.operation == old.operation)
                    && (old.event_id != new.event_id
                        || old.key_version != new.key_version
                        || old.random_position > new.random_position)
                {
                    return Err(StoreError::Invalid("NPC source invocation rewind"));
                }
            }
        }
    }
    Ok(version)
}
pub(super) async fn commit(
    tx: &mut Transaction<'_, Postgres>,
    update: &NpcWorkflowUpdate,
    value: &NpcWorkflowSaveV3,
    version: u64,
) -> Result<(), StoreError> {
    let rows=sqlx::query("INSERT INTO npc_source_heads(source_id,version,invocation,workflow_version,world_epoch,content_generation,completed,checkpoint,archived,source_cell) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$10,$11) ON CONFLICT(source_id) DO UPDATE SET version=EXCLUDED.version,invocation=EXCLUDED.invocation,workflow_version=EXCLUDED.workflow_version,world_epoch=EXCLUDED.world_epoch,content_generation=EXCLUDED.content_generation,completed=EXCLUDED.completed,checkpoint=EXCLUDED.checkpoint,archived=EXCLUDED.archived,source_cell=EXCLUDED.source_cell WHERE npc_source_heads.version=$9")
        .bind(i64::from(value.source)).bind(version as i64).bind(update.invocation.as_slice()).bind(update.expected_version+1).bind(update.world_epoch as i64).bind(value.content_generation.as_slice()).bind(value.completed).bind(&update.checkpoint).bind(version as i64-1).bind(value.archive.is_some()).bind(value.location.as_ref().map(|p|i64::from(p.cell))).execute(&mut **tx).await?.rows_affected();
    if rows != 1 {
        return Err(StoreError::Conflict(value.source));
    }
    Ok(())
}
