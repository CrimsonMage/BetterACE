use crate::{PgStore, StoreError};
use bace_persistence::StoredAggregate;
use sqlx::Row;
impl PgStore {
    /// Complete bounded durable dropped-item set for a cell. Off-thread only;
    /// caller still admits physical geometry before making any item visible.
    pub async fn load_world_items(
        &self,
        cell: u32,
        max_items: usize,
        max_bytes: usize,
    ) -> Result<Vec<StoredAggregate>, StoreError> {
        if cell == 0 || max_items > 4096 || max_bytes > 64 * 1024 * 1024 {
            return Err(StoreError::Invalid("world item load limits"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        let sizes=sqlx::query("SELECT s.object_id,octet_length(s.payload) AS bytes FROM item_places p JOIN entity_snapshots s ON s.object_id=p.item_id WHERE p.kind=1 AND p.cell_id=$1 ORDER BY s.object_id LIMIT $2").bind(i64::from(cell)).bind((max_items+1) as i64).fetch_all(&mut *tx).await?;
        if sizes.len() > max_items {
            return Err(StoreError::Invalid("world item count"));
        }
        let mut total = 0usize;
        let mut ids = Vec::with_capacity(sizes.len());
        for row in sizes {
            total = total
                .checked_add(
                    usize::try_from(row.get::<i32, _>("bytes"))
                        .map_err(|_| StoreError::Invalid("world item length"))?,
                )
                .ok_or(StoreError::Invalid("world item bytes"))?;
            if total > max_bytes {
                return Err(StoreError::Invalid("world item bytes"));
            }
            let id = row.get::<i64, _>("object_id");
            crate::ownership::lock_object(&mut tx, id as u32).await?;
            ids.push(id);
        }
        let rows=sqlx::query("SELECT object_id,version,payload FROM entity_snapshots WHERE object_id=ANY($1) ORDER BY object_id").bind(ids).fetch_all(&mut *tx).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let value = StoredAggregate {
                object_id: row.get::<i64, _>("object_id") as u32,
                persisted_version: row.get("version"),
                bytes: row.get("payload"),
            };
            let (id, placement) = crate::placements::decode_placed_snapshot(&value.bytes)?;
            if id != value.object_id
                || !matches!(placement,bace_storage_codec::ItemPlacementV2::World(ref p) if p.obj_cell_id==cell)
            {
                return Err(StoreError::OwnershipConflict);
            }
            result.push(value);
        }
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(result)
    }
}
