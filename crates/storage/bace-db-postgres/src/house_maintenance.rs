//! Bounded cold housing pages; offline rent never instantiates physics actors.
use crate::{PgStore, StoreError};
use bace_persistence::{CharacterLease, HouseMaintenanceSnapshot, OwnershipState, StoredAggregate};
use sqlx::Row;
use std::collections::BTreeSet;
impl PgStore {
    pub async fn house_maintenance_page(
        &self,
        after: u32,
        limit: u16,
    ) -> Result<Vec<HouseMaintenanceSnapshot>, StoreError> {
        if limit == 0 || limit > 128 {
            return Err(StoreError::Invalid("housing maintenance page"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        let rows=sqlx::query("SELECT h.object_id,h.owner_id,octet_length(e.payload) AS bytes FROM house_ownership h JOIN entity_snapshots e ON e.object_id=h.object_id WHERE h.object_id>$1 ORDER BY h.object_id LIMIT $2").bind(i64::from(after)).bind(i32::from(limit)).fetch_all(&mut *tx).await?;
        let mut ids = Vec::with_capacity(rows.len());
        let mut locks = BTreeSet::new();
        let mut bytes = 0usize;
        for row in rows {
            let id = row.get::<i64, _>("object_id");
            ids.push(id);
            locks.insert(id);
            if let Some(owner) = row.get::<Option<i64>, _>("owner_id") {
                locks.insert(owner);
            }
            let size = usize::try_from(row.get::<i32, _>("bytes"))
                .map_err(|_| StoreError::Invalid("house length"))?;
            bytes = bytes
                .checked_add(size)
                .filter(|n| *n <= 64 * 1024 * 1024)
                .ok_or(StoreError::Invalid("housing maintenance bytes"))?;
            if size > 2 * 1024 * 1024 + 52 {
                return Err(StoreError::Invalid("housing snapshot length"));
            }
        }
        for id in locks {
            crate::ownership::lock_object(&mut tx, id as u32).await?;
        }
        let rows=sqlx::query("SELECT h.object_id,h.owner_id,h.access_generation,e.version,e.payload,o.epoch,o.state FROM house_ownership h JOIN entity_snapshots e ON e.object_id=h.object_id LEFT JOIN character_ownership o ON o.character_id=h.owner_id WHERE h.object_id=ANY($1) ORDER BY h.object_id").bind(ids).fetch_all(&mut *tx).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.get::<i64, _>("object_id") as u32;
            let aggregate = StoredAggregate {
                object_id: id,
                persisted_version: row.get("version"),
                bytes: row.get("payload"),
            };
            let house = bace_storage_codec::HouseSaveV3::decode_migrate(&aggregate.bytes)
                .map_err(|_| StoreError::Invalid("housing maintenance snapshot"))?;
            let owner = row.get::<Option<i64>, _>("owner_id");
            if house.owner_id.map(i64::from) != owner
                || house.access_generation as i64 != row.get::<i64, _>("access_generation")
            {
                return Err(StoreError::OwnershipConflict);
            }
            let owner = owner
                .map(|id| {
                    let state = match row.get::<String, _>("state").as_str() {
                        "offline" => OwnershipState::Offline,
                        "loading" => OwnershipState::Loading,
                        "online" => OwnershipState::Online,
                        "logging_out" => OwnershipState::LoggingOut,
                        _ => return Err(StoreError::OwnershipConflict),
                    };
                    Ok(CharacterLease {
                        character_id: id as u32,
                        epoch: row.get("epoch"),
                        state,
                    })
                })
                .transpose()?;
            result.push(HouseMaintenanceSnapshot { aggregate, owner });
        }
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(result)
    }
}
