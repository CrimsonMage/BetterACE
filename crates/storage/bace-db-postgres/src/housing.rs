use crate::{PgStore, StoreError};
use bace_persistence::{CharacterLease, OperationOutcome, OwnershipState, SaveSnapshot};
use bace_storage_codec::HouseSaveV1;
use sha2::{Digest, Sha256};
use sqlx::Row;

impl PgStore {
    /// Save/create housing state under its owner's current online or offline fence.
    /// Ownership transfer requires a separate multi-participant operation; this
    /// method cannot silently change the owner of an existing house.
    pub async fn save_house(
        &self,
        operation_id: &str,
        owner: CharacterLease,
        house: &HouseSaveV1,
        expected_version: i64,
    ) -> Result<OperationOutcome, StoreError> {
        if operation_id.is_empty()
            || operation_id.len() > 128
            || owner.character_id != house.owner_id
            || owner.epoch < 0
            || !matches!(
                owner.state,
                OwnershipState::Offline | OwnershipState::Online
            )
        {
            return Err(StoreError::OwnershipConflict);
        }
        let snapshot = SaveSnapshot {
            object_id: house.entity.object_id,
            mutation_revision: house.entity.mutation_revision,
            expected_version,
            bytes: house
                .encode()
                .map_err(|_| StoreError::Invalid("frozen housing save"))?,
        };
        crate::writes::validate(std::slice::from_ref(&snapshot))?;
        let mut hash = Sha256::new();
        hash.update(b"betterace-housing-save-v1");
        hash.update(owner.epoch.to_le_bytes());
        hash.update([u8::from(owner.state == OwnershipState::Online)]);
        hash.update(expected_version.to_le_bytes());
        hash.update(&snapshot.bytes);
        let fingerprint: [u8; 32] = hash.finalize().into();
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO durable_operations(operation_id,request_fingerprint) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(operation_id).bind(fingerprint.as_slice()).execute(&mut *tx).await?.rows_affected();
        if inserted == 0 {
            let old: Vec<u8> = sqlx::query_scalar(
                "SELECT request_fingerprint FROM durable_operations WHERE operation_id=$1",
            )
            .bind(operation_id)
            .fetch_one(&mut *tx)
            .await?;
            if old != fingerprint {
                return Err(StoreError::OperationMismatch);
            }
            tx.commit().await.map_err(crate::store::commit_error)?;
            return Ok(OperationOutcome::AlreadyCommitted);
        }
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        let mut ids = [owner.character_id, house.entity.object_id];
        ids.sort_unstable();
        for id in ids {
            crate::ownership::lock_object(&mut tx, id).await?;
        }
        crate::ownership::check_lease(&mut tx, owner).await?;
        let old = sqlx::query(
            "SELECT owner_id,house_id FROM house_ownership WHERE object_id=$1 FOR UPDATE",
        )
        .bind(i64::from(house.entity.object_id))
        .fetch_optional(&mut *tx)
        .await?;
        match old {
            Some(old)
                if old.get::<Option<i64>, _>("owner_id") == Some(i64::from(owner.character_id))
                    && old.get::<i64, _>("house_id") == i64::from(house.house_id) => {}
            None if expected_version == 0 => {}
            _ => return Err(StoreError::OwnershipConflict),
        }
        let acks =
            crate::writes::write_all_unchecked(&mut tx, std::slice::from_ref(&snapshot)).await?;
        sqlx::query("INSERT INTO house_ownership(object_id,house_id,owner_id) VALUES($1,$2,$3) ON CONFLICT(object_id) DO NOTHING")
            .bind(i64::from(house.entity.object_id)).bind(i64::from(house.house_id)).bind(i64::from(owner.character_id))
            .execute(&mut *tx).await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(OperationOutcome::Committed(acks))
    }
}
