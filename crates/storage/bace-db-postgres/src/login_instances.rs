//! Durable source login count, separate from ownership epochs and save revisions.
use crate::{PgStore, StoreError};
use bace_persistence::{CharacterLease, OnlineLoginReceipt, OwnershipState};
use sqlx::Row;
impl PgStore {
    pub async fn finish_login_receipt(
        &self,
        lease: CharacterLease,
    ) -> Result<OnlineLoginReceipt, StoreError> {
        if lease.state != OwnershipState::Loading {
            return Err(StoreError::OwnershipConflict);
        }
        let mut tx = self.pool.begin().await?;
        crate::ownership::check_lease(&mut tx, lease).await?;
        let row = sqlx::query("UPDATE character_ownership SET state='online',total_logins=total_logins+1 WHERE character_id=$1 AND total_logins<2147483647 RETURNING total_logins")
            .bind(i64::from(lease.character_id)).fetch_optional(&mut *tx).await?
            .ok_or(StoreError::Invalid("character login counter exhausted"))?;
        let total_logins = u32::try_from(row.get::<i32, _>("total_logins"))
            .map_err(|_| StoreError::Invalid("character login counter"))?;
        let receipt = OnlineLoginReceipt {
            lease: CharacterLease {
                state: OwnershipState::Online,
                ..lease
            },
            total_logins,
        };
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(receipt)
    }
    /// Serializes behind an uncertain writer and validates its exact Online lease.
    /// A retry resolves this receipt instead of incrementing a second time.
    pub async fn online_login_receipt(
        &self,
        lease: CharacterLease,
    ) -> Result<OnlineLoginReceipt, StoreError> {
        if lease.state != OwnershipState::Online {
            return Err(StoreError::OwnershipConflict);
        }
        let mut tx = self.pool.begin().await?;
        crate::ownership::check_lease(&mut tx, lease).await?;
        let row = sqlx::query("SELECT total_logins FROM character_ownership WHERE character_id=$1")
            .bind(i64::from(lease.character_id))
            .fetch_one(&mut *tx)
            .await?;
        let total_logins = u32::try_from(row.get::<i32, _>("total_logins"))
            .ok()
            .filter(|count| *count > 0)
            .ok_or(StoreError::Invalid("missing Online login counter"))?;
        tx.commit().await?;
        Ok(OnlineLoginReceipt {
            lease,
            total_logins,
        })
    }
}
