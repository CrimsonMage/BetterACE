use crate::{PgStore, StoreError};
use bace_persistence::{OperationOutcome, SaveAck, SaveSnapshot, StoredAggregate};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
impl PgStore {
    pub async fn load(&self, object_id: u32) -> Result<Option<StoredAggregate>, StoreError> {
        let row = sqlx::query("SELECT version,payload FROM entity_snapshots WHERE object_id=$1")
            .bind(i64::from(object_id))
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| StoredAggregate {
            object_id,
            persisted_version: r.get("version"),
            bytes: r.get("payload"),
        }))
    }
    /// All-or-nothing bounded batch; callers retain dirty snapshots until acknowledged.
    pub async fn save_batch(&self, snapshots: &[SaveSnapshot]) -> Result<Vec<SaveAck>, StoreError> {
        validate(snapshots)?;
        let mut tx = self.pool.begin().await?;
        let acks = write_all(&mut tx, snapshots).await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(acks)
    }
    /// Reserve touched aggregates before submission. On ambiguous commit failure, resolve ID.
    pub async fn valuable(
        &self,
        operation_id: &str,
        snapshots: &[SaveSnapshot],
    ) -> Result<OperationOutcome, StoreError> {
        validate(snapshots)?;
        let fingerprint = request_fingerprint(snapshots);
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
        let acks = write_all(&mut tx, snapshots).await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(OperationOutcome::Committed(acks))
    }
    /// None means no committed operation is currently visible; retry the same ID/request,
    /// never issue a replacement ID while an earlier commit might still be in progress.
    pub async fn resolve_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(sqlx::query_scalar(
            "SELECT request_fingerprint FROM durable_operations WHERE operation_id=$1",
        )
        .bind(operation_id)
        .fetch_optional(&self.pool)
        .await?)
    }
}
pub(crate) fn validate(snapshots: &[SaveSnapshot]) -> Result<(), StoreError> {
    if snapshots.is_empty() || snapshots.len() > 1024 {
        return Err(StoreError::Invalid("snapshot batch count"));
    }
    let total_bytes = snapshots
        .iter()
        .try_fold(0_usize, |sum, s| sum.checked_add(s.bytes.len()));
    if total_bytes.is_none_or(|sum| sum > 64 * 1024 * 1024) {
        return Err(StoreError::Invalid("snapshot batch exceeds 64 MiB"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for snapshot in snapshots {
        if !ids.insert(snapshot.object_id)
            || snapshot.expected_version < 0
            || snapshot.expected_version == i64::MAX
        {
            return Err(StoreError::Invalid(
                "duplicate object or invalid CAS version",
            ));
        }
    }
    Ok(())
}
pub(crate) async fn write_all(
    tx: &mut Transaction<'_, Postgres>,
    snapshots: &[SaveSnapshot],
) -> Result<Vec<SaveAck>, StoreError> {
    let mut ids: Vec<_> = snapshots.iter().map(|s| s.object_id).collect();
    ids.sort_unstable();
    for id in ids {
        crate::ownership::lock_object(tx, id).await?;
        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM character_ownership WHERE character_id=$1 UNION ALL SELECT 1 FROM item_ownership WHERE item_id=$1 OR container_id=$1 UNION ALL SELECT 1 FROM house_ownership WHERE object_id=$1 UNION ALL SELECT 1 FROM item_places WHERE item_id=$1)",
        )
        .bind(i64::from(id))
        .fetch_one(&mut **tx)
        .await?;
        if owned {
            return Err(StoreError::OwnershipConflict);
        }
    }
    write_all_unchecked(tx, snapshots).await
}
pub(crate) async fn write_all_unchecked(
    tx: &mut Transaction<'_, Postgres>,
    snapshots: &[SaveSnapshot],
) -> Result<Vec<SaveAck>, StoreError> {
    write_all_inner(tx, snapshots, false).await
}
pub(crate) async fn write_world_death_unchecked(
    tx: &mut Transaction<'_, Postgres>,
    snapshots: &[SaveSnapshot],
) -> Result<Vec<SaveAck>, StoreError> {
    write_all_inner(tx, snapshots, true).await
}
async fn write_all_inner(
    tx: &mut Transaction<'_, Postgres>,
    snapshots: &[SaveSnapshot],
    allow_death_marker: bool,
) -> Result<Vec<SaveAck>, StoreError> {
    if !allow_death_marker
        && snapshots.iter().any(|snapshot| {
            snapshot.bytes.len() >= 12
                && snapshot.bytes.starts_with(b"ACERBIN\0")
                && u16::from_le_bytes([snapshot.bytes[10], snapshot.bytes[11]])
                    == bace_storage_codec::PVE_DEATH_RECEIPT_KIND
        })
    {
        return Err(StoreError::Invalid(
            "PVE marker needs world death operation",
        ));
    }
    // Even paths which already checked ownership must preserve embedded identity.
    // Validate the entire batch before the first mutation/receipt can commit.
    crate::snapshot_identity::validate(tx, snapshots).await?;
    // Stable lock order avoids deadlock between overlapping valuable operations.
    let mut sorted: Vec<_> = snapshots.iter().collect();
    sorted.sort_by_key(|s| s.object_id);
    let mut acks = Vec::with_capacity(sorted.len());
    for snapshot in sorted {
        let count = if snapshot.expected_version == 0 {
            sqlx::query("INSERT INTO entity_snapshots(object_id,version,payload) VALUES($1,1,$2) ON CONFLICT DO NOTHING")
                .bind(i64::from(snapshot.object_id)).bind(&snapshot.bytes).execute(&mut **tx).await?.rows_affected()
        } else {
            sqlx::query("UPDATE entity_snapshots SET version=version+1,payload=$3 WHERE object_id=$1 AND version=$2")
                .bind(i64::from(snapshot.object_id)).bind(snapshot.expected_version).bind(&snapshot.bytes).execute(&mut **tx).await?.rows_affected()
        };
        if count != 1 {
            return Err(StoreError::Conflict(snapshot.object_id));
        }
        acks.push(SaveAck {
            object_id: snapshot.object_id,
            mutation_revision: snapshot.mutation_revision,
            persisted_version: snapshot.expected_version + 1,
        });
    }
    Ok(acks)
}

fn request_fingerprint(snapshots: &[SaveSnapshot]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"ace-valuable-v1");
    let mut sorted: Vec<_> = snapshots.iter().collect();
    sorted.sort_by_key(|s| s.object_id);
    for snapshot in sorted {
        hash.update(snapshot.object_id.to_le_bytes());
        hash.update(snapshot.mutation_revision.to_le_bytes());
        hash.update(snapshot.expected_version.to_le_bytes());
        hash.update((snapshot.bytes.len() as u64).to_le_bytes());
        hash.update(&snapshot.bytes);
    }
    hash.finalize().into()
}
