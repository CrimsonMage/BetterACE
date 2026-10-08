//! Source-head inventory proof and coherent current custody reads. Later durable
//! transfers/tombstones win over a checkpoint's earlier item beforeimages.
use crate::{PgStore, StoreError};
use bace_persistence::{DurableItemPlace, LocatedSnapshot, StoredAggregate};
use bace_storage_codec::{ItemSaveV5, NpcWorkflowSaveV3};
use sqlx::{PgConnection, Postgres, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};
async fn forest(
    connection: &mut PgConnection,
    source: u32,
) -> Result<Vec<LocatedSnapshot>, StoreError> {
    let invalid = || StoreError::Invalid("NPC inventory forest bounds/identity");
    let mut frontier = vec![i64::from(source)];
    let mut seen = BTreeSet::from([source]);
    let mut metadata = BTreeMap::new();
    let mut total = 0usize;
    let mut depth = 0u16;
    while !frontier.is_empty() {
        depth += 1;
        if depth > 64 {
            return Err(invalid());
        }
        let rows=sqlx::query("SELECT o.item_id,o.container_id,o.slot,o.pack_slot,o.equipped,e.version,octet_length(e.payload) AS bytes FROM item_ownership o JOIN entity_snapshots e ON e.object_id=o.item_id WHERE o.container_id=ANY($1) ORDER BY o.container_id,o.slot LIMIT $2").bind(&frontier).bind((1025-metadata.len())as i64).fetch_all(&mut *connection).await?;
        if rows.len() + metadata.len() > 1024 {
            return Err(invalid());
        }
        frontier.clear();
        for row in rows {
            let id = u32::try_from(row.get::<i64, _>("item_id")).map_err(|_| invalid())?;
            if !seen.insert(id) {
                return Err(invalid());
            }
            let length = usize::try_from(row.get::<i32, _>("bytes")).map_err(|_| invalid())?;
            total = total.checked_add(length).ok_or_else(invalid)?;
            if length == 0 || length > 2 * 1024 * 1024 + 52 || total > 64 * 1024 * 1024 {
                return Err(invalid());
            }
            let placement = DurableItemPlace::Contained {
                container: u32::try_from(row.get::<i64, _>("container_id"))
                    .map_err(|_| invalid())?,
                slot: u32::try_from(row.get::<i64, _>("slot")).map_err(|_| invalid())?,
                pack_slot: row.get("pack_slot"),
                equipped: u32::try_from(row.get::<i64, _>("equipped")).map_err(|_| invalid())?,
            };
            metadata.insert(id, (placement, depth, row.get::<i64, _>("version"), length));
            frontier.push(i64::from(id));
        }
    }
    let ids: Vec<_> = metadata.keys().map(|id| i64::from(*id)).collect();
    let rows=sqlx::query("SELECT object_id,version,payload FROM entity_snapshots WHERE object_id=ANY($1) ORDER BY object_id").bind(ids).fetch_all(&mut *connection).await?;
    if rows.len() != metadata.len() {
        return Err(invalid());
    }
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id = u32::try_from(row.get::<i64, _>("object_id")).map_err(|_| invalid())?;
        let (placement, depth, version, length) = metadata.remove(&id).ok_or_else(invalid)?;
        let bytes: Vec<u8> = row.get("payload");
        if bytes.len() != length || row.get::<i64, _>("version") != version {
            return Err(invalid());
        }
        let item = ItemSaveV5::decode_or_migrate(&bytes, None).map_err(|_| invalid())?;
        let expected = match item.placement {
            bace_storage_codec::ItemPlacementV2::Contained {
                container,
                slot,
                pack_slot,
                equipped,
            } => DurableItemPlace::Contained {
                container,
                slot,
                pack_slot,
                equipped,
            },
            _ => return Err(invalid()),
        };
        if item.entity.object_id != id || placement != expected {
            return Err(invalid());
        }
        out.push(LocatedSnapshot {
            aggregate: StoredAggregate {
                object_id: id,
                persisted_version: version,
                bytes,
            },
            placement,
            depth,
        });
    }
    out.sort_by_key(|s| (s.depth, s.aggregate.object_id));
    Ok(out)
}
pub(super) async fn validate_committed(
    tx: &mut Transaction<'_, Postgres>,
    value: &NpcWorkflowSaveV3,
) -> Result<(), StoreError> {
    let Some(proof) = &value.inventory else {
        return Ok(());
    };
    if proof.source_persisted_version <= 0 {
        return Err(StoreError::Invalid("NPC source aggregate fence"));
    }
    let root = sqlx::query("SELECT version,payload FROM entity_snapshots WHERE object_id=$1")
        .bind(i64::from(value.source))
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::Conflict(value.source))?;
    if root.get::<i64, _>("version") != proof.source_persisted_version {
        return Err(StoreError::Conflict(value.source));
    }
    let bytes: Vec<u8> = root.get("payload");
    let entity = if let Ok(saved) = bace_storage_codec::EntitySaveV1::decode_item(&bytes) {
        saved
    } else {
        ItemSaveV5::decode_or_migrate(&bytes, None)
            .map_err(|_| StoreError::Invalid("NPC root aggregate kind"))?
            .previous
            .previous
            .previous
            .entity
    };
    if entity.object_id != value.source
        || entity.mutation_revision != proof.source_mutation_revision
    {
        return Err(StoreError::Conflict(value.source));
    }
    let current = forest(&mut *tx, value.source).await?;
    if current.len() != proof.items.len() {
        return Err(StoreError::Invalid(
            "NPC source inventory incomplete journal",
        ));
    }
    for item in &proof.items {
        let row = current
            .iter()
            .find(|r| r.aggregate.object_id == item.id)
            .ok_or(StoreError::Conflict(item.id))?;
        if item.persisted_version <= 0
            || row.aggregate.persisted_version != item.persisted_version
            || row.placement
                != (DurableItemPlace::Contained {
                    container: item.container,
                    slot: item.slot,
                    pack_slot: item.pack_slot,
                    equipped: item.equipped,
                })
        {
            return Err(StoreError::Conflict(item.id));
        }
        let decoded = ItemSaveV5::decode_or_migrate(&row.aggregate.bytes, None)
            .map_err(|_| StoreError::Invalid("NPC source item payload"))?;
        if decoded.entity.mutation_revision != item.revision {
            return Err(StoreError::Conflict(item.id));
        }
    }
    Ok(())
}
impl PgStore {
    pub async fn load_npc_source_inventory(
        &self,
        source: u32,
        source_version: u64,
    ) -> Result<Vec<LocatedSnapshot>, StoreError> {
        if source == 0 || source_version == 0 || source_version > i64::MAX as u64 {
            return Err(StoreError::Invalid("NPC source inventory head"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query(
            "SELECT version,checkpoint FROM npc_source_heads WHERE source_id=$1 FOR SHARE",
        )
        .bind(i64::from(source))
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::Conflict(source))?;
        if row.get::<i64, _>("version") != source_version as i64 {
            return Err(StoreError::Conflict(source));
        }
        let value = NpcWorkflowSaveV3::decode(&row.get::<Vec<u8>, _>("checkpoint"))
            .map_err(|_| StoreError::Invalid("NPC source inventory checkpoint"))?;
        let proof = value.inventory.as_ref().ok_or(StoreError::Invalid(
            "NPC source has no durable inventory proof",
        ))?;
        let rows = forest(&mut tx, source).await?;
        for item in &proof.items {
            let version: Option<i64> =
                sqlx::query_scalar("SELECT version FROM entity_snapshots WHERE object_id=$1")
                    .bind(i64::from(item.id))
                    .fetch_optional(&mut *tx)
                    .await?;
            if item.persisted_version <= 0 || version.is_none_or(|v| v < item.persisted_version) {
                return Err(StoreError::Conflict(item.id));
            }
        }
        tx.commit().await?;
        Ok(rows)
    }
}
