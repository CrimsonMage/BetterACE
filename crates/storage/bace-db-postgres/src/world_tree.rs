//! One coherent world-cell inventory forest, including nested bags and corpses.
use crate::{PgStore, StoreError};
use bace_persistence::{
    DurableItemPlace as Place, InventoryLoadLimits, LocatedSnapshot, StoredAggregate,
};
use sqlx::Row;
use std::collections::BTreeMap;
impl PgStore {
    pub async fn load_world_item_tree(
        &self,
        cell: u32,
        limits: InventoryLoadLimits,
    ) -> Result<Vec<LocatedSnapshot>, StoreError> {
        if cell == 0
            || limits.max_items > 4096
            || limits.max_depth == 0
            || limits.max_depth > 64
            || limits.max_total_bytes > 64 * 1024 * 1024
        {
            return Err(StoreError::Invalid("world inventory limits"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        let roots=sqlx::query("SELECT s.object_id,s.version,octet_length(s.payload) AS bytes FROM item_places p JOIN entity_snapshots s ON s.object_id=p.item_id WHERE p.kind=1 AND p.cell_id=$1 ORDER BY s.object_id LIMIT $2").bind(i64::from(cell)).bind((limits.max_items+1) as i64).fetch_all(&mut *tx).await?;
        let mut metadata = BTreeMap::new();
        let mut frontier = Vec::new();
        let mut bytes = 0usize;
        for row in roots {
            let id = row.get::<i64, _>("object_id") as u32;
            bytes = charge(bytes, row.get("bytes"), limits)?;
            metadata.insert(
                id,
                (Place::World { cell }, 0u16, row.get::<i64, _>("version")),
            );
            frontier.push(i64::from(id));
        }
        if metadata.len() > limits.max_items {
            return Err(StoreError::Invalid("world inventory count"));
        }
        let mut depth = 0u16;
        while !frontier.is_empty() {
            depth += 1;
            let rows=sqlx::query("SELECT s.object_id,s.version,octet_length(s.payload) AS bytes,o.container_id,o.slot,o.pack_slot,o.equipped FROM item_ownership o JOIN entity_snapshots s ON s.object_id=o.item_id WHERE o.container_id=ANY($1) ORDER BY s.object_id LIMIT $2").bind(&frontier).bind((limits.max_items-metadata.len()+1) as i64).fetch_all(&mut *tx).await?;
            if rows.is_empty() {
                break;
            }
            if usize::from(depth) > limits.max_depth
                || rows.len() > limits.max_items - metadata.len()
            {
                return Err(StoreError::Invalid("world inventory depth or count"));
            }
            frontier.clear();
            for row in rows {
                let id = row.get::<i64, _>("object_id") as u32;
                let location = Place::Contained {
                    container: row.get::<i64, _>("container_id") as u32,
                    slot: row.get::<i64, _>("slot") as u32,
                    pack_slot: row.get("pack_slot"),
                    equipped: row.get::<i64, _>("equipped") as u32,
                };
                bytes = charge(bytes, row.get("bytes"), limits)?;
                if metadata
                    .insert(id, (location, depth, row.get::<i64, _>("version")))
                    .is_some()
                {
                    return Err(StoreError::Invalid("world inventory cycle"));
                }
                frontier.push(i64::from(id));
            }
        }
        for id in metadata.keys() {
            crate::ownership::lock_object(&mut tx, *id).await?;
        }
        let ids: Vec<i64> = metadata.keys().map(|id| i64::from(*id)).collect();
        let rows=sqlx::query("SELECT object_id,version,payload FROM entity_snapshots WHERE object_id=ANY($1) ORDER BY object_id").bind(ids).fetch_all(&mut *tx).await?;
        if rows.len() != metadata.len() {
            return Err(StoreError::Invalid("world inventory missing snapshot"));
        }
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let aggregate = StoredAggregate {
                object_id: row.get::<i64, _>("object_id") as u32,
                persisted_version: row.get("version"),
                bytes: row.get("payload"),
            };
            let (placement, depth, version) = metadata
                .remove(&aggregate.object_id)
                .ok_or(StoreError::Invalid("world inventory identity"))?;
            if version != aggregate.persisted_version {
                return Err(StoreError::Conflict(aggregate.object_id));
            }
            let info = bace_storage_codec::inspect(
                &aggregate.bytes,
                bace_storage_codec::CodecLimits {
                    max_payload_bytes: 2 * 1024 * 1024,
                },
            )
            .map_err(|_| StoreError::Invalid("world inventory envelope"))?;
            if (info.kind, info.schema_version) == (101, 1) {
                let old = bace_storage_codec::EntitySaveV1::decode_item(&aggregate.bytes)
                    .map_err(|_| StoreError::Invalid("world inventory V1"))?;
                if old.object_id != aggregate.object_id
                    || !matches!(placement, Place::Contained { .. })
                {
                    return Err(StoreError::OwnershipConflict);
                }
            } else {
                let (id, encoded) = crate::placements::decode_placed_snapshot(&aggregate.bytes)?;
                if id != aggregate.object_id || crate::placements::dto_place(&encoded) != placement
                {
                    return Err(StoreError::OwnershipConflict);
                }
            }
            result.push(LocatedSnapshot {
                aggregate,
                placement,
                depth,
            });
        }
        result.sort_by_key(|s| (s.depth, s.aggregate.object_id));
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(result)
    }
}
fn charge(current: usize, size: i32, limits: InventoryLoadLimits) -> Result<usize, StoreError> {
    let size = usize::try_from(size).map_err(|_| StoreError::Invalid("world inventory size"))?;
    if size == 0 || size > 2 * 1024 * 1024 + 52 {
        return Err(StoreError::Invalid("world inventory record size"));
    }
    current
        .checked_add(size)
        .filter(|n| *n <= limits.max_total_bytes)
        .ok_or(StoreError::Invalid("world inventory byte budget"))
}
