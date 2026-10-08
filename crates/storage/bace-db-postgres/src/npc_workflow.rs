use crate::{PgStore, StoreError};
mod heads;
mod inventory;
use bace_persistence::{NpcStageOperation, NpcWorkflowUpdate, OperationOutcome, StoredNpcWorkflow};
use bace_storage_codec::NpcWorkflowSaveV3;
pub use heads::StoredNpcSourceHead;
use sqlx::{Postgres, Row, Transaction};
impl PgStore {
    pub async fn npc_stage(
        &self,
        operation: &NpcStageOperation,
    ) -> Result<OperationOutcome, StoreError> {
        crate::placements::execute(
            self,
            &operation.inventory,
            None,
            Some(&operation.workflow),
            None,
            None,
        )
        .await
    }
    /// Keyset page for restart recovery; no query loads the whole world's scripts.
    pub async fn pending_npc_workflows(
        &self,
        source: u32,
        after: Option<[u8; 16]>,
        limit: u32,
    ) -> Result<Vec<StoredNpcWorkflow>, StoreError> {
        if source == 0 || !(1..=16).contains(&limit) {
            return Err(StoreError::Invalid("NPC workflow page"));
        }
        let Some(head) = self.npc_source_head(source).await? else {
            return Ok(vec![]);
        };
        if head.completed || after.is_some_and(|after| head.invocation <= after) {
            return Ok(vec![]);
        }
        Ok(vec![StoredNpcWorkflow {
            invocation: head.invocation,
            version: head.workflow_version,
            checkpoint: head.checkpoint,
        }])
    }
}
pub(crate) fn validate(update: &NpcWorkflowUpdate) -> Result<NpcWorkflowSaveV3, StoreError> {
    if update.world_epoch == 0
        || update.world_epoch > i64::MAX as u64
        || update.expected_version < 0
        || update.expected_version == i64::MAX
    {
        return Err(StoreError::Invalid("NPC checkpoint version"));
    }
    let value = NpcWorkflowSaveV3::decode_or_migrate(&update.checkpoint)
        .map_err(|_| StoreError::Invalid("NPC checkpoint payload"))?;
    if value.invocation != update.invocation || value.stage > i64::MAX as u64 {
        return Err(StoreError::Invalid("NPC checkpoint identity"));
    }
    Ok(value)
}
pub(crate) async fn commit(
    tx: &mut Transaction<'_, Postgres>,
    update: &NpcWorkflowUpdate,
) -> Result<(), StoreError> {
    let value = validate(update)?;
    let source_version = heads::prepare(tx, update, &value).await?;
    let previous =
        sqlx::query("SELECT version,checkpoint FROM npc_workflows WHERE invocation=$1 FOR UPDATE")
            .bind(update.invocation.as_slice())
            .fetch_optional(&mut **tx)
            .await?;
    match previous {
        None if update.expected_version == 0 => {
            if value.stage != 0 {
                return Err(StoreError::Invalid("initial NPC stage must be zero"));
            }
            sqlx::query("INSERT INTO npc_workflows(invocation,source_id,version,stage,program_hash,content_generation,key_version,completed,checkpoint) VALUES($1,$2,1,$3,$4,$5,$6,$7,$8)")
                .bind(value.invocation.as_slice()).bind(i64::from(value.source)).bind(value.stage as i64).bind(value.program_hash.as_slice()).bind(value.content_generation.as_slice()).bind(i64::from(value.key_version)).bind(value.completed).bind(&update.checkpoint).execute(&mut **tx).await?;
        }
        Some(row) if row.get::<i64, _>("version") == update.expected_version => {
            let previous =
                NpcWorkflowSaveV3::decode_or_migrate(&row.get::<Vec<u8>, _>("checkpoint"))
                    .map_err(|_| StoreError::Invalid("prior NPC checkpoint"))?;
            if previous.completed
                || previous.source != value.source
                || previous.program_hash != value.program_hash
                || previous.content_generation != value.content_generation
                || previous.key_version != value.key_version
                || previous.active_operation > value.active_operation
                || previous.stage.checked_add(1) != Some(value.stage)
                || previous.logical_now > value.logical_now
                || previous.next_order > value.next_order
                || previous.remaining_instructions < value.remaining_instructions
            {
                return Err(StoreError::Invalid(
                    "NPC checkpoint identity change or rewind",
                ));
            }
            for old in &previous.invocations {
                if let Some(new) = value
                    .invocations
                    .iter()
                    .find(|i| i.operation == old.operation)
                    && (old.event_id != new.event_id
                        || old.key_version != new.key_version
                        || old.random_position > new.random_position)
                {
                    return Err(StoreError::Invalid("NPC invocation identity or RNG rewind"));
                }
            }
            sqlx::query("UPDATE npc_workflows SET version=version+1,stage=$2,completed=$3,checkpoint=$4 WHERE invocation=$1")
                .bind(value.invocation.as_slice()).bind(value.stage as i64).bind(value.completed).bind(&update.checkpoint).execute(&mut **tx).await?;
        }
        _ => return Err(StoreError::Conflict(value.source)),
    }
    heads::commit(tx, update, &value, source_version).await
}

pub(crate) async fn validate_committed_inventory(
    tx: &mut Transaction<'_, Postgres>,
    update: &NpcWorkflowUpdate,
) -> Result<(), StoreError> {
    let value = validate(update)?;
    inventory::validate_committed(tx, &value).await
}
