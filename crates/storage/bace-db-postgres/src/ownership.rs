use crate::{PgStore, StoreError};
use bace_persistence::{
    CharacterLease, CharacterLoad, OwnershipState, SaveAck, SaveSnapshot, StoredAggregate,
};
use sqlx::{Postgres, Row, Transaction};

pub(crate) async fn lock_object(
    tx: &mut Transaction<'_, Postgres>,
    id: u32,
) -> Result<(), StoreError> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(8_589_934_592_i64 + i64::from(id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub(crate) async fn check_lease(
    tx: &mut Transaction<'_, Postgres>,
    lease: CharacterLease,
) -> Result<(), StoreError> {
    lock_object(tx, lease.character_id).await?;
    let row =
        sqlx::query("SELECT epoch,state FROM character_ownership WHERE character_id=$1 FOR UPDATE")
            .bind(i64::from(lease.character_id))
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::OwnershipConflict)?;
    if row.get::<i64, _>("epoch") != lease.epoch
        || row.get::<String, _>("state") != lease.state.as_str()
    {
        return Err(StoreError::OwnershipConflict);
    }
    Ok(())
}
impl PgStore {
    /// Establishes a fresh character and its offline ownership atomically. No implicit takeover.
    pub async fn create_owned_character(
        &self,
        snapshot: &SaveSnapshot,
    ) -> Result<CharacterLease, StoreError> {
        if snapshot.expected_version != 0 {
            return Err(StoreError::Invalid("fresh character CAS must be zero"));
        }
        let mut tx = self.pool.begin().await?;
        crate::writes::validate(std::slice::from_ref(snapshot))?;
        crate::writes::write_all(&mut tx, std::slice::from_ref(snapshot)).await?;
        sqlx::query("INSERT INTO character_ownership(character_id) VALUES($1)")
            .bind(i64::from(snapshot.object_id))
            .execute(&mut *tx)
            .await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(CharacterLease {
            character_id: snapshot.object_id,
            epoch: 0,
            state: OwnershipState::Offline,
        })
    }
    /// Fences offline writers before loading the snapshot; no read-before-fence race.
    pub async fn begin_login(&self, offline: CharacterLease) -> Result<CharacterLoad, StoreError> {
        if offline.state != OwnershipState::Offline {
            return Err(StoreError::OwnershipConflict);
        }
        let mut tx = self.pool.begin().await?;
        check_lease(&mut tx, offline).await?;
        let epoch = offline
            .epoch
            .checked_add(1)
            .ok_or(StoreError::OwnershipConflict)?;
        sqlx::query(
            "UPDATE character_ownership SET epoch=$2,state='loading' WHERE character_id=$1",
        )
        .bind(i64::from(offline.character_id))
        .bind(epoch)
        .execute(&mut *tx)
        .await?;
        let row=sqlx::query("SELECT e.version,e.payload,o.cached_xp::text AS cached FROM entity_snapshots e JOIN character_ownership o ON o.character_id=e.object_id WHERE e.object_id=$1").bind(i64::from(offline.character_id)).fetch_one(&mut *tx).await?;
        let load = CharacterLoad {
            lease: CharacterLease {
                epoch,
                state: OwnershipState::Loading,
                ..offline
            },
            snapshot: StoredAggregate {
                object_id: offline.character_id,
                persisted_version: row.get("version"),
                bytes: row.get("payload"),
            },
            cached_xp: row
                .get::<String, _>("cached")
                .parse()
                .map_err(|_| StoreError::Invalid("cached XP"))?,
        };
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(load)
    }
    pub async fn finish_login(&self, lease: CharacterLease) -> Result<CharacterLease, StoreError> {
        self.transition(
            lease,
            OwnershipState::Loading,
            OwnershipState::Online,
            false,
        )
        .await
    }
    pub async fn abort_loading(&self, lease: CharacterLease) -> Result<CharacterLease, StoreError> {
        self.transition(
            lease,
            OwnershipState::Loading,
            OwnershipState::Offline,
            true,
        )
        .await
    }
    pub async fn begin_logout(&self, lease: CharacterLease) -> Result<CharacterLease, StoreError> {
        self.transition(
            lease,
            OwnershipState::Online,
            OwnershipState::LoggingOut,
            true,
        )
        .await
    }
    async fn transition(
        &self,
        lease: CharacterLease,
        from: OwnershipState,
        to: OwnershipState,
        increment: bool,
    ) -> Result<CharacterLease, StoreError> {
        if lease.state != from {
            return Err(StoreError::OwnershipConflict);
        }
        let mut tx = self.pool.begin().await?;
        check_lease(&mut tx, lease).await?;
        let epoch = lease
            .epoch
            .checked_add(i64::from(increment))
            .ok_or(StoreError::OwnershipConflict)?;
        sqlx::query("UPDATE character_ownership SET epoch=$2,state=$3 WHERE character_id=$1")
            .bind(i64::from(lease.character_id))
            .bind(epoch)
            .bind(to.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(CharacterLease {
            epoch,
            state: to,
            ..lease
        })
    }
    pub async fn save_owned(
        &self,
        lease: CharacterLease,
        snapshot: &SaveSnapshot,
    ) -> Result<SaveAck, StoreError> {
        if lease.state != OwnershipState::Online || snapshot.object_id != lease.character_id {
            return Err(StoreError::OwnershipConflict);
        }
        crate::writes::validate(std::slice::from_ref(snapshot))?;
        let mut tx = self.pool.begin().await?;
        check_lease(&mut tx, lease).await?;
        let ack = crate::writes::write_all_unchecked(&mut tx, std::slice::from_ref(snapshot))
            .await?
            .remove(0);
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(ack)
    }
    /// Snapshot MUST include logout skill values. Failed commits retain LoggingOut ownership.
    pub async fn finish_logout(
        &self,
        lease: CharacterLease,
        snapshot: &SaveSnapshot,
    ) -> Result<(CharacterLease, SaveAck), StoreError> {
        if lease.state != OwnershipState::LoggingOut || snapshot.object_id != lease.character_id {
            return Err(StoreError::OwnershipConflict);
        }
        crate::writes::validate(std::slice::from_ref(snapshot))?;
        let mut tx = self.pool.begin().await?;
        check_lease(&mut tx, lease).await?;
        let ack = crate::writes::write_all_unchecked(&mut tx, std::slice::from_ref(snapshot))
            .await?
            .remove(0);
        sqlx::query("UPDATE character_ownership SET state='offline' WHERE character_id=$1")
            .bind(i64::from(lease.character_id))
            .execute(&mut *tx)
            .await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok((
            CharacterLease {
                state: OwnershipState::Offline,
                ..lease
            },
            ack,
        ))
    }
    /// Reconcile uncertain transitions without changing state. Restart takeover requires explicit orchestration.
    pub async fn character_lease(&self, id: u32) -> Result<Option<CharacterLease>, StoreError> {
        let row = sqlx::query("SELECT epoch,state FROM character_ownership WHERE character_id=$1")
            .bind(i64::from(id))
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| {
            Ok(CharacterLease {
                character_id: id,
                epoch: r.get("epoch"),
                state: match r.get::<String, _>("state").as_str() {
                    "offline" => OwnershipState::Offline,
                    "loading" => OwnershipState::Loading,
                    "online" => OwnershipState::Online,
                    "logging_out" => OwnershipState::LoggingOut,
                    _ => return Err(StoreError::OwnershipConflict),
                },
            })
        })
        .transpose()
    }
}
