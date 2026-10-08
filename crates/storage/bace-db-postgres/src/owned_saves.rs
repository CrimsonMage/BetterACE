//! Fenced routine aggregate writes share inventory's hierarchy sequencer without
//! a durable operation receipt per five-second save. Uncertain CAS commits must
//! be reconciled from stored revisions before retrying; they never clear dirtiness.
use crate::{PgStore, StoreError};
use bace_persistence::{OwnedSaveBatch, OwnershipState, SaveAck};
use std::collections::BTreeSet;
impl PgStore {
    pub async fn save_owned_batch(
        &self,
        batch: &OwnedSaveBatch,
    ) -> Result<Vec<SaveAck>, StoreError> {
        crate::writes::validate(&batch.snapshots)?;
        if batch.participants.is_empty()
            || batch.participants.len() > 1024
            || batch.leases.len() > 1024
        {
            return Err(StoreError::Invalid("owned batch bounds"));
        }
        let participants: BTreeSet<_> = batch.participants.iter().copied().collect();
        if participants.len() != batch.participants.len()
            || participants.contains(&0)
            || batch.snapshots.iter().any(|s| {
                s.expected_version == 0
                    || s.bytes.is_empty()
                    || s.bytes.len() > 17 * 1024 * 1024
                    || !participants.contains(&s.object_id)
            })
        {
            return Err(StoreError::Invalid(
                "owned batch participant or fresh aggregate",
            ));
        }
        let mut leased = BTreeSet::new();
        for lease in &batch.leases {
            if !participants.contains(&lease.character_id)
                || !leased.insert(lease.character_id)
                || lease.epoch < 0
                || !matches!(
                    lease.state,
                    OwnershipState::Online | OwnershipState::Offline
                )
            {
                return Err(StoreError::OwnershipConflict);
            }
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        for id in &participants {
            crate::ownership::lock_object(&mut tx, *id).await?;
        }
        let ids: Vec<_> = participants.iter().map(|id| i64::from(*id)).collect();
        let characters: Vec<i64> = sqlx::query_scalar(
            "SELECT character_id FROM character_ownership WHERE character_id=ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await?;
        let characters: BTreeSet<_> = characters.into_iter().map(|id| id as u32).collect();
        if characters != leased {
            return Err(StoreError::OwnershipConflict);
        }
        for lease in &batch.leases {
            crate::ownership::check_lease(&mut tx, *lease).await?;
        }
        crate::inventory::check_ancestries(&mut tx, &participants).await?;
        let acks = crate::writes::write_all_unchecked(&mut tx, &batch.snapshots).await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(acks)
    }
}

impl PgStore {
    /// Serialize behind the original writer before resolving an uncertain routine CAS.
    /// None is a confirmed unchanged version, never an unlocked observation of an in-flight write.
    pub async fn resolve_owned_routine(
        &self,
        lease: bace_persistence::CharacterLease,
        request: &bace_persistence::SaveSnapshot,
    ) -> Result<Option<SaveAck>, StoreError> {
        use sqlx::Row;
        crate::writes::validate(std::slice::from_ref(request))?;
        if request.object_id != lease.character_id
            || request.expected_version <= 0
            || request.bytes.is_empty()
            || request.bytes.len() > 17 * 1024 * 1024
            || !matches!(
                lease.state,
                OwnershipState::Online | OwnershipState::Offline
            )
        {
            return Err(StoreError::Invalid("routine resolution identity"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        crate::ownership::lock_object(&mut tx, request.object_id).await?;
        crate::ownership::check_lease(&mut tx, lease).await?;
        let row = sqlx::query("SELECT version,payload FROM entity_snapshots WHERE object_id=$1")
            .bind(i64::from(request.object_id))
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::Conflict(request.object_id))?;
        let version: i64 = row.try_get("version")?;
        let bytes: Vec<u8> = row.try_get("payload")?;
        let result = if version == request.expected_version {
            None
        } else if request.expected_version.checked_add(1) == Some(version) && bytes == request.bytes
        {
            Some(SaveAck {
                object_id: request.object_id,
                mutation_revision: request.mutation_revision,
                persisted_version: version,
            })
        } else {
            return Err(StoreError::Conflict(request.object_id));
        };
        tx.commit().await?;
        Ok(result)
    }
}
impl PgStore {
    /// Resolve an exact atomic owned batch under the original hierarchy sequencer.
    /// Mixed old/new rows or any differing payload are conflicts, never partial success.
    pub async fn resolve_owned_batch(
        &self,
        batch: &OwnedSaveBatch,
    ) -> Result<Option<Vec<SaveAck>>, StoreError> {
        use sqlx::Row;
        crate::writes::validate(&batch.snapshots)?;
        let ids: BTreeSet<_> = batch.participants.iter().copied().collect();
        if ids.len() != batch.participants.len()
            || ids.is_empty()
            || ids.len() > 1024
            || ids.contains(&0)
            || batch.snapshots.iter().any(|s| {
                s.expected_version <= 0
                    || s.bytes.is_empty()
                    || s.bytes.len() > 17 * 1024 * 1024
                    || !ids.contains(&s.object_id)
            })
        {
            return Err(StoreError::Invalid("owned resolution bounds"));
        }
        let mut leases = BTreeSet::new();
        for lease in &batch.leases {
            if !ids.contains(&lease.character_id)
                || !leases.insert(lease.character_id)
                || !matches!(
                    lease.state,
                    OwnershipState::Online | OwnershipState::Offline
                )
            {
                return Err(StoreError::OwnershipConflict);
            }
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        for id in &ids {
            crate::ownership::lock_object(&mut tx, *id).await?;
        }
        let query_ids: Vec<_> = ids.iter().map(|id| i64::from(*id)).collect();
        let characters: Vec<i64> = sqlx::query_scalar(
            "SELECT character_id FROM character_ownership WHERE character_id=ANY($1)",
        )
        .bind(&query_ids)
        .fetch_all(&mut *tx)
        .await?;
        if characters
            .into_iter()
            .map(|id| id as u32)
            .collect::<BTreeSet<_>>()
            != leases
        {
            return Err(StoreError::OwnershipConflict);
        }
        for lease in &batch.leases {
            crate::ownership::check_lease(&mut tx, *lease).await?;
        }
        crate::inventory::check_ancestries(&mut tx, &ids).await?;
        let mut committed = None;
        let mut acks = Vec::with_capacity(batch.snapshots.len());
        for s in &batch.snapshots {
            let row =
                sqlx::query("SELECT version,payload FROM entity_snapshots WHERE object_id=$1")
                    .bind(i64::from(s.object_id))
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or(StoreError::Conflict(s.object_id))?;
            let version: i64 = row.try_get("version")?;
            let bytes: Vec<u8> = row.try_get("payload")?;
            let wrote = if version == s.expected_version {
                false
            } else if Some(version) == s.expected_version.checked_add(1) && bytes == s.bytes {
                true
            } else {
                return Err(StoreError::Conflict(s.object_id));
            };
            if committed.is_some_and(|before| before != wrote) {
                return Err(StoreError::Conflict(s.object_id));
            }
            committed = Some(wrote);
            if wrote {
                acks.push(SaveAck {
                    object_id: s.object_id,
                    mutation_revision: s.mutation_revision,
                    persisted_version: version,
                });
            }
        }
        tx.commit().await?;
        Ok(committed.unwrap_or(false).then_some(acks))
    }
}
