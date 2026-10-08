//! Staff gag snapshots share the character lease lock with login and logout.
use crate::{PgStore, StoreError};
use bace_persistence::{
    CharacterLease, OfflineStaffPlayer, OwnershipState, SaveAck, StaffGagOperation,
    StaffGagReceipt, StoredAggregate,
};
use bace_storage_codec::PlayerSaveV6;
use sha2::{Digest, Sha256};
use sqlx::Row;
impl PgStore {
    /// One indexed, bounded offline before-image. The write rechecks this lease.
    pub async fn load_offline_staff_player(
        &self,
        character: u32,
    ) -> Result<OfflineStaffPlayer, StoreError> {
        if character == 0 {
            return Err(StoreError::Invalid("staff target identity"));
        }
        let mut tx = self.pool.begin().await?;
        crate::ownership::lock_object(&mut tx, character).await?;
        let row=sqlx::query("SELECT o.epoch,o.state,e.version,octet_length(e.payload) AS bytes FROM character_ownership o JOIN entity_snapshots e ON e.object_id=o.character_id WHERE o.character_id=$1 FOR UPDATE OF o,e").bind(i64::from(character)).fetch_optional(&mut *tx).await?.ok_or(StoreError::OwnershipConflict)?;
        if row.get::<String, _>("state") != "offline" {
            return Err(StoreError::OwnershipConflict);
        }
        if !(1..=17 * 1024 * 1024).contains(&row.get::<i32, _>("bytes")) {
            return Err(StoreError::Invalid("staff target payload bounds"));
        }
        let bytes = sqlx::query_scalar("SELECT payload FROM entity_snapshots WHERE object_id=$1")
            .bind(i64::from(character))
            .fetch_one(&mut *tx)
            .await?;
        let result = OfflineStaffPlayer {
            lease: CharacterLease {
                character_id: character,
                epoch: row.get("epoch"),
                state: OwnershipState::Offline,
            },
            snapshot: StoredAggregate {
                object_id: character,
                persisted_version: row.get("version"),
                bytes,
            },
        };
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(result)
    }
    pub async fn apply_staff_gag(
        &self,
        operation: &StaffGagOperation,
    ) -> Result<StaffGagReceipt, StoreError> {
        let digest = fingerprint(operation)?;
        let mut tx = self.pool.begin().await?;
        let key = i32::from_le_bytes(
            operation.operation_id[..4]
                .try_into()
                .expect("fixed operation ID"),
        );
        sqlx::query("SELECT pg_advisory_xact_lock(42812020,$1)")
            .bind(key)
            .execute(&mut *tx)
            .await?;
        if let Some(row)=sqlx::query("SELECT fingerprint,character_id,mutation_revision::text AS revision,persisted_version FROM staff_gag_operations WHERE operation_id=$1").bind(operation.operation_id.as_slice()).fetch_optional(&mut *tx).await? {
            if row.get::<Vec<u8>,_>("fingerprint")!=digest {return Err(StoreError::OperationMismatch);}
            let receipt=StaffGagReceipt{operation_id:operation.operation_id,acknowledgement:SaveAck{object_id:u32::try_from(row.get::<i64,_>("character_id")).map_err(|_|StoreError::Invalid("staff receipt identity"))?,mutation_revision:row.get::<String,_>("revision").parse().map_err(|_|StoreError::Invalid("staff receipt revision"))?,persisted_version:row.get("persisted_version")}};
            tx.commit().await.map_err(crate::store::commit_error)?;
            return Ok(receipt);
        }
        crate::ownership::check_lease(&mut tx, operation.lease).await?;
        let candidate = PlayerSaveV6::decode(&operation.snapshot.bytes)
            .map_err(|_| StoreError::Invalid("staff gag player snapshot"))?;
        validate_properties(operation, &candidate)?;
        if operation.lease.state == OwnershipState::Offline {
            let row=sqlx::query("SELECT version,octet_length(payload) AS bytes FROM entity_snapshots WHERE object_id=$1 FOR UPDATE").bind(i64::from(operation.lease.character_id)).fetch_one(&mut *tx).await?;
            if row.get::<i64, _>("version") != operation.snapshot.expected_version {
                return Err(StoreError::Conflict(operation.lease.character_id));
            }
            if !(1..=17 * 1024 * 1024).contains(&row.get::<i32, _>("bytes")) {
                return Err(StoreError::Invalid("staff prior payload bounds"));
            }
            let bytes: Vec<u8> =
                sqlx::query_scalar("SELECT payload FROM entity_snapshots WHERE object_id=$1")
                    .bind(i64::from(operation.lease.character_id))
                    .fetch_one(&mut *tx)
                    .await?;
            let before = PlayerSaveV6::decode_or_migrate(&bytes)
                .map_err(|_| StoreError::Invalid("staff prior player"))?;
            validate_offline_change(before, candidate)?;
        }
        let acknowledgement =
            crate::writes::write_all_unchecked(&mut tx, std::slice::from_ref(&operation.snapshot))
                .await?
                .remove(0);
        sqlx::query("INSERT INTO staff_gag_operations(operation_id,fingerprint,issuer_account,character_id,mutation_revision,persisted_version) VALUES($1,$2,$3,$4,$5::text::numeric,$6)").bind(operation.operation_id.as_slice()).bind(digest.as_slice()).bind(operation.issuer_account as i64).bind(i64::from(operation.lease.character_id)).bind(acknowledgement.mutation_revision.to_string()).bind(acknowledgement.persisted_version).execute(&mut *tx).await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(StaffGagReceipt {
            operation_id: operation.operation_id,
            acknowledgement,
        })
    }
}
fn fingerprint(op: &StaffGagOperation) -> Result<[u8; 32], StoreError> {
    if op.operation_id == [0; 16]
        || op.issuer_account == 0
        || op.issuer_account > i64::MAX as u64
        || op.lease.character_id == 0
        || op.lease.epoch < 0
        || !matches!(
            op.lease.state,
            OwnershipState::Offline | OwnershipState::Online
        )
        || op.snapshot.object_id != op.lease.character_id
        || !op.unix_seconds.is_finite()
        || op.unix_seconds < 0.
    {
        return Err(StoreError::Invalid("staff gag operation"));
    }
    crate::writes::validate(std::slice::from_ref(&op.snapshot))?;
    let mut hash = Sha256::new();
    hash.update(b"BetterACE-staff-gag-v1");
    hash.update(op.issuer_account.to_le_bytes());
    hash.update(op.lease.character_id.to_le_bytes());
    hash.update(op.lease.epoch.to_le_bytes());
    hash.update(op.lease.state.as_str().as_bytes());
    hash.update([u8::from(op.enabled)]);
    hash.update(op.unix_seconds.to_bits().to_le_bytes());
    hash.update(op.snapshot.mutation_revision.to_le_bytes());
    hash.update(op.snapshot.expected_version.to_le_bytes());
    hash.update(&op.snapshot.bytes);
    Ok(hash.finalize().into())
}
fn validate_properties(op: &StaffGagOperation, player: &PlayerSaveV6) -> Result<(), StoreError> {
    let properties = &player.player.entity.state.properties;
    let active = properties
        .bools
        .iter()
        .find(|p| p.id == 111)
        .map(|p| p.value);
    let timestamp = properties
        .floats
        .iter()
        .find(|p| p.id == 112)
        .map(|p| p.value);
    let duration = properties
        .floats
        .iter()
        .find(|p| p.id == 161)
        .map(|p| p.value);
    if player.player.entity.mutation_revision != op.snapshot.mutation_revision
        || if op.enabled {
            active != Some(true) || timestamp != Some(op.unix_seconds) || duration != Some(300.)
        } else {
            active.is_some() || timestamp.is_some() || duration.is_some()
        }
    {
        return Err(StoreError::Invalid("staff gag property projection"));
    }
    Ok(())
}
fn validate_offline_change(
    mut before: PlayerSaveV6,
    mut after: PlayerSaveV6,
) -> Result<(), StoreError> {
    if before.player.entity.mutation_revision.checked_add(1)
        != Some(after.player.entity.mutation_revision)
    {
        return Err(StoreError::Invalid("staff offline revision"));
    }
    after.player.entity.mutation_revision = before.player.entity.mutation_revision;
    for player in [&mut before, &mut after] {
        player
            .player
            .entity
            .state
            .properties
            .bools
            .retain(|p| p.id != 111);
        player
            .player
            .entity
            .state
            .properties
            .floats
            .retain(|p| ![112, 161].contains(&p.id));
    }
    if before != after {
        return Err(StoreError::Invalid("staff offline mutation scope"));
    }
    Ok(())
}
