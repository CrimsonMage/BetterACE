//! Coherent bounded descendant loading. Hierarchy and character fences are held
//! until all lengths and snapshots have been checked; partial loads never escape.
use crate::{PgStore, StoreError};
use bace_persistence::{
    CharacterLease, InventoryLoadLimits, LoadedInventory, OwnershipState, StoredAggregate,
    StoredInventoryItem,
};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};
impl PgStore {
    pub async fn load_character_inventory(
        &self,
        lease: CharacterLease,
        limits: InventoryLoadLimits,
    ) -> Result<LoadedInventory, StoreError> {
        if lease.state != OwnershipState::Loading
            || lease.epoch < 0
            || limits.max_items > 1024
            || limits.max_depth == 0
            || limits.max_depth > 64
            || limits.max_total_bytes > 64 * 1024 * 1024
        {
            return Err(StoreError::Invalid("inventory load bounds or lease"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        crate::ownership::check_lease(&mut tx, lease).await?;
        let mut frontier = vec![i64::from(lease.character_id)];
        let mut seen = BTreeSet::from([lease.character_id]);
        let mut metadata = BTreeMap::new();
        let mut total = 0usize;
        let mut depth = 0usize;
        while !frontier.is_empty() {
            depth += 1;
            let limit = limits.max_items.saturating_sub(metadata.len()) + 1;
            // Discover only bounded scalar metadata first; a corrupt large payload
            // cannot make an unbounded driver allocation before the byte check.
            let rows=sqlx::query("SELECT o.item_id,o.container_id,o.slot,o.pack_slot,o.equipped,e.version,octet_length(e.payload) AS bytes FROM item_ownership o JOIN entity_snapshots e ON e.object_id=o.item_id WHERE o.container_id=ANY($1) ORDER BY o.container_id,o.slot LIMIT $2")
                .bind(&frontier).bind(limit as i64).fetch_all(&mut *tx).await?;
            if rows.is_empty() {
                break;
            }
            if depth > limits.max_depth
                || rows.len() > limits.max_items.saturating_sub(metadata.len())
            {
                return Err(StoreError::Invalid("inventory depth or item limit"));
            }
            frontier.clear();
            for row in rows {
                let id = u32::try_from(row.get::<i64, _>("item_id"))
                    .map_err(|_| StoreError::Invalid("item ID"))?;
                if !seen.insert(id) {
                    return Err(StoreError::Invalid("inventory cycle or duplicate"));
                }
                let bytes = usize::try_from(row.get::<i32, _>("bytes"))
                    .map_err(|_| StoreError::Invalid("item payload length"))?;
                total = total
                    .checked_add(bytes)
                    .ok_or(StoreError::Invalid("inventory bytes"))?;
                if bytes == 0 || bytes > 17 * 1024 * 1024 || total > limits.max_total_bytes {
                    return Err(StoreError::Invalid("inventory byte limit"));
                }
                let location = crate::inventory::location(&row)?;
                metadata.insert(
                    id,
                    (
                        location,
                        depth as u16,
                        row.get::<i64, _>("version"),
                        bytes,
                        row.get::<bool, _>("pack_slot"),
                        row.get::<i64, _>("equipped") as u32,
                    ),
                );
                frontier.push(i64::from(id));
            }
        }
        // All hierarchy participants are locked before fetching payloads. Ordinary
        // fenced routine writers and transfers use the same hierarchy sequencer.
        for id in metadata.keys() {
            crate::ownership::lock_object(&mut tx, *id).await?;
        }
        let ids: Vec<_> = metadata.keys().map(|id| i64::from(*id)).collect();
        let rows=sqlx::query("SELECT object_id,version,payload FROM entity_snapshots WHERE object_id=ANY($1) ORDER BY object_id").bind(ids).fetch_all(&mut *tx).await?;
        if rows.len() != metadata.len() {
            return Err(StoreError::Invalid("inventory snapshot missing"));
        }
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.get::<i64, _>("object_id") as u32;
            let (location, depth, version, length, pack_slot, equipped) = metadata
                .remove(&id)
                .ok_or(StoreError::Invalid("inventory identity"))?;
            let bytes: Vec<u8> = row.get("payload");
            if bytes.len() != length || row.get::<i64, _>("version") != version {
                return Err(StoreError::Invalid("inventory changed while loading"));
            }
            items.push(StoredInventoryItem {
                aggregate: StoredAggregate {
                    object_id: id,
                    persisted_version: version,
                    bytes,
                },
                location,
                depth,
                pack_slot,
                equipped,
            });
        }
        items.sort_by_key(|i| {
            (
                i.depth,
                i.location.container,
                i.location.slot,
                i.aggregate.object_id,
            )
        });
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(LoadedInventory { lease, items })
    }
}
