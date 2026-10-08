use crate::{PgStore, StoreError};
use sqlx::{Connection, PgConnection};

/// A dedicated PostgreSQL connection holds the process's exclusive world lock.
/// Dropping it closes the connection; it is never returned to a pool with a lock.
pub struct WorldOwner {
    connection: PgConnection,
    epoch: u64,
}

impl PgStore {
    pub async fn acquire_world_owner(&self) -> Result<WorldOwner, StoreError> {
        let mut connection = self.pool.acquire().await?.detach();
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(42812002)")
            .fetch_one(&mut connection)
            .await?;
        if !acquired {
            return Err(StoreError::OwnershipConflict);
        }
        let epoch:Option<i64>=sqlx::query_scalar("UPDATE world_execution_epoch SET epoch=epoch+1 WHERE singleton AND epoch<9223372036854775807 RETURNING epoch").fetch_optional(&mut connection).await?;
        let epoch = epoch.ok_or(StoreError::Invalid("world execution epoch exhausted"))? as u64;
        Ok(WorldOwner { connection, epoch })
    }
}

impl WorldOwner {
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Startup-only crash recovery after exclusive world ownership is established.
    /// Batches fence every previous lease, including offline jobs, while retaining
    /// the last durable snapshot. This cannot restore unsaved crash-lost memory.
    pub async fn recover_characters(&mut self) -> Result<u64, StoreError> {
        let mut after = 0_i64;
        let mut count = 0_u64;
        loop {
            let mut tx = self.connection.begin().await?;
            let ids: Vec<i64> = sqlx::query_scalar("SELECT character_id FROM character_ownership WHERE character_id>$1 ORDER BY character_id LIMIT 1024")
                .bind(after).fetch_all(&mut *tx).await?;
            if ids.is_empty() {
                tx.rollback().await?;
                return Ok(count);
            }
            for id in &ids {
                crate::ownership::lock_object(&mut tx, *id as u32).await?;
                let changed = sqlx::query("UPDATE character_ownership SET epoch=epoch+1,state='offline' WHERE character_id=$1 AND epoch<9223372036854775807")
                    .bind(id).execute(&mut *tx).await?.rows_affected();
                if changed != 1 {
                    return Err(StoreError::OwnershipConflict);
                }
            }
            tx.commit().await.map_err(crate::store::commit_error)?;
            after = *ids
                .last()
                .ok_or(StoreError::Invalid("empty recovery batch"))?;
            count += ids.len() as u64;
        }
    }

    /// Call from lifecycle I/O, never from simulation. A failure requires stopping
    /// new world admission and recovering/draining the existing simulation owner.
    pub async fn check(&mut self) -> Result<(), StoreError> {
        sqlx::query("SELECT 1")
            .execute(&mut self.connection)
            .await?;
        Ok(())
    }

    pub async fn close(mut self) -> Result<(), StoreError> {
        let released: bool = sqlx::query_scalar("SELECT pg_advisory_unlock(42812002)")
            .fetch_one(&mut self.connection)
            .await?;
        if !released {
            return Err(StoreError::OwnershipConflict);
        }
        self.connection.close().await?;
        Ok(())
    }
}
