//! Coherent bounded descendant loading. Hierarchy and character fences are held
//! until all lengths and snapshots have been checked; partial loads never escape.
use crate::{PgStore, StoreError};
use bace_persistence::{
    CharacterLease, InventoryLoadLimits, LoadedInventory, OwnershipState, StoredAggregate,
    StoredInventoryItem,
};
use sha2::{Digest, Sha256};
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
        let mut creation_repairs = Vec::new();
        let mut pack_repairs = Vec::new();
        for row in rows {
            let id = row.get::<i64, _>("object_id") as u32;
            let (location, depth, version, length, pack_slot, mut equipped) = metadata
                .remove(&id)
                .ok_or(StoreError::Invalid("inventory identity"))?;
            let bytes: Vec<u8> = row.get("payload");
            if bytes.len() != length || row.get::<i64, _>("version") != version {
                return Err(StoreError::Invalid("inventory changed while loading"));
            }
            // Character creation before the equipped-column fix stored the
            // frozen V1 clothing marker but left the relational location at
            // zero. Repair only untouched direct creation items under this
            // Loading fence and the hierarchy lock.
            if depth == 1
                && location.container == lease.character_id
                && version == 1
                && !pack_slot
                && equipped == 0
                && let Ok(saved) = bace_storage_codec::EntitySaveV1::decode_item(&bytes)
            {
                let expected = crate::players::creation_equipped(&saved, lease.character_id)?;
                if expected != 0 {
                    creation_repairs.push((id, expected));
                    equipped = expected;
                }
            }
            if depth == 1
                && location.container == lease.character_id
                && version == 1
                && !pack_slot
                && equipped == 0
                && let Ok(saved) = bace_storage_codec::EntitySaveV1::decode_item(&bytes)
                && crate::players::creation_pack_slot(&saved)
            {
                pack_repairs.push(id);
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
        if !pack_repairs.is_empty() {
            // Preserve already placed packs and the original starter order.
            let mut next = items
                .iter()
                .filter(|item| item.location.container == lease.character_id && item.pack_slot)
                .map(|item| item.location.slot)
                .max()
                .map_or(Ok(0), |slot| {
                    slot.checked_add(1)
                        .ok_or(StoreError::Invalid("pack slot overflow"))
                })?;
            pack_repairs.sort_by_key(|id| {
                items
                    .iter()
                    .find(|item| item.aggregate.object_id == *id)
                    .map_or(u32::MAX, |item| item.location.slot)
            });
            let mut hash = Sha256::new();
            hash.update(b"betterace-creation-pack-repair-v1");
            hash.update(lease.character_id.to_le_bytes());
            for id in &pack_repairs {
                hash.update(id.to_le_bytes());
            }
            let fingerprint: [u8; 32] = hash.finalize().into();
            let operation_id = format!("creation-pack-repair:{}", lease.character_id);
            let inserted = sqlx::query("INSERT INTO durable_operations(operation_id,request_fingerprint) VALUES($1,$2) ON CONFLICT DO NOTHING")
                .bind(&operation_id).bind(fingerprint.as_slice()).execute(&mut *tx).await?.rows_affected();
            if inserted != 1 {
                return Err(StoreError::Invalid("creation pack repair receipt reused"));
            }
            for id in pack_repairs {
                let updated = sqlx::query("UPDATE item_ownership SET slot=$1,pack_slot=true WHERE item_id=$2 AND container_id=$3 AND pack_slot=false AND equipped=0")
                    .bind(i64::from(next)).bind(i64::from(id)).bind(i64::from(lease.character_id))
                    .execute(&mut *tx).await?.rows_affected();
                if updated != 1 {
                    return Err(StoreError::Conflict(id));
                }
                let item = items
                    .iter_mut()
                    .find(|item| item.aggregate.object_id == id)
                    .ok_or(StoreError::Invalid("creation pack repair identity"))?;
                item.location.slot = next;
                item.pack_slot = true;
                next = next
                    .checked_add(1)
                    .ok_or(StoreError::Invalid("pack slot overflow"))?;
            }
            items.sort_by_key(|i| {
                (
                    i.depth,
                    i.location.container,
                    i.pack_slot,
                    i.location.slot,
                    i.aggregate.object_id,
                )
            });
        }
        if !creation_repairs.is_empty() {
            let mut hash = Sha256::new();
            hash.update(b"betterace-creation-equipment-repair-v1");
            hash.update(lease.character_id.to_le_bytes());
            for (id, equipped) in &creation_repairs {
                hash.update(id.to_le_bytes());
                hash.update(equipped.to_le_bytes());
            }
            let fingerprint: [u8; 32] = hash.finalize().into();
            let operation_id = format!("creation-equipment-repair:{}", lease.character_id);
            let inserted = sqlx::query("INSERT INTO durable_operations(operation_id,request_fingerprint) VALUES($1,$2) ON CONFLICT DO NOTHING")
                .bind(&operation_id)
                .bind(fingerprint.as_slice())
                .execute(&mut *tx)
                .await?
                .rows_affected();
            if inserted != 1 {
                return Err(StoreError::Invalid(
                    "creation equipment repair receipt reused",
                ));
            }
            for (id, equipped) in creation_repairs {
                let updated = sqlx::query("UPDATE item_ownership SET equipped=$1 WHERE item_id=$2 AND container_id=$3 AND equipped=0 AND pack_slot=false")
                    .bind(i64::from(equipped))
                    .bind(i64::from(id))
                    .bind(i64::from(lease.character_id))
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
                if updated != 1 {
                    return Err(StoreError::Conflict(id));
                }
            }
        }
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(LoadedInventory { lease, items })
    }
}
