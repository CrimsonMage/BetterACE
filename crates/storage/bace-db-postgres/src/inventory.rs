use crate::{PgStore, StoreError};
use bace_persistence::{InventoryOperation, ItemLocation, OperationOutcome, OwnershipState};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeSet;

impl PgStore {
    pub async fn item_location(&self, item: u32) -> Result<Option<ItemLocation>, StoreError> {
        let row = sqlx::query("SELECT container_id,slot FROM item_ownership WHERE item_id=$1")
            .bind(i64::from(item))
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| location(&row)).transpose()
    }

    /// Atomic fenced inventory/quest/vendor transaction. Gameplay validation and
    /// participant reservations must precede this call; success is a durable ack.
    pub async fn inventory_operation(
        &self,
        operation: &InventoryOperation,
    ) -> Result<OperationOutcome, StoreError> {
        let participants = validate(operation)?;
        let fingerprint = fingerprint(operation);
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO durable_operations(operation_id,request_fingerprint) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(&operation.operation_id).bind(fingerprint.as_slice())
            .execute(&mut *tx).await?.rows_affected();
        if inserted == 0 {
            let existing: Vec<u8> = sqlx::query_scalar(
                "SELECT request_fingerprint FROM durable_operations WHERE operation_id=$1",
            )
            .bind(&operation.operation_id)
            .fetch_one(&mut *tx)
            .await?;
            if existing != fingerprint {
                return Err(StoreError::OperationMismatch);
            }
            tx.commit().await.map_err(crate::store::commit_error)?;
            return Ok(OperationOutcome::AlreadyCommitted);
        }
        // One bounded transaction sequencer makes hierarchy changes serializable.
        // No simulation thread waits on this lock. Object locks follow sorted order.
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        let moved: Vec<i64> = operation
            .transfers
            .iter()
            .map(|c| i64::from(c.item))
            .collect();
        let tracked: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM item_places WHERE item_id=ANY($1))")
                .bind(moved)
                .fetch_one(&mut *tx)
                .await?;
        if tracked {
            return Err(StoreError::Invalid(
                "V2 items require explicit placement transaction",
            ));
        }
        for id in &participants {
            crate::ownership::lock_object(&mut tx, *id).await?;
        }
        for id in &participants {
            let owned: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM character_ownership WHERE character_id=$1)",
            )
            .bind(i64::from(*id))
            .fetch_one(&mut *tx)
            .await?;
            match operation
                .leases
                .iter()
                .find(|lease| lease.character_id == *id)
            {
                Some(lease) if owned => {
                    if operation
                        .transfers
                        .iter()
                        .any(|transfer| transfer.item == *id)
                    {
                        return Err(StoreError::Invalid(
                            "a character cannot be an inventory item",
                        ));
                    }
                    crate::ownership::check_lease(&mut tx, *lease).await?;
                }
                None if !owned => {}
                _ => return Err(StoreError::OwnershipConflict),
            }
        }
        check_ancestries(&mut tx, &participants).await?;
        for transfer in &operation.transfers {
            let row = sqlx::query(
                "SELECT container_id,slot FROM item_ownership WHERE item_id=$1 FOR UPDATE",
            )
            .bind(i64::from(transfer.item))
            .fetch_optional(&mut *tx)
            .await?;
            if row.as_ref().map(location).transpose()? != transfer.expected {
                return Err(StoreError::Conflict(transfer.item));
            }
        }
        let acks = crate::writes::write_all_unchecked(&mut tx, &operation.snapshots).await?;
        for transfer in &operation.transfers {
            match transfer.destination {
                Some(destination) => {
                    sqlx::query("INSERT INTO item_ownership(item_id,container_id,slot) VALUES($1,$2,$3) ON CONFLICT(item_id) DO UPDATE SET container_id=EXCLUDED.container_id,slot=EXCLUDED.slot")
                        .bind(i64::from(transfer.item)).bind(i64::from(destination.container))
                        .bind(i64::from(destination.slot)).execute(&mut *tx).await?;
                }
                None => {
                    sqlx::query("DELETE FROM item_ownership WHERE item_id=$1")
                        .bind(i64::from(transfer.item))
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
        check_ancestries(&mut tx, &participants).await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(OperationOutcome::Committed(acks))
    }
}

pub(crate) fn location(row: &sqlx::postgres::PgRow) -> Result<ItemLocation, StoreError> {
    Ok(ItemLocation {
        container: u32::try_from(row.get::<i64, _>("container_id"))
            .map_err(|_| StoreError::Invalid("container ID"))?,
        slot: u32::try_from(row.get::<i64, _>("slot"))
            .map_err(|_| StoreError::Invalid("item slot"))?,
    })
}

pub(crate) async fn check_ancestries(
    tx: &mut Transaction<'_, Postgres>,
    participants: &BTreeSet<u32>,
) -> Result<(), StoreError> {
    // Two bounded index reads, not a database round-trip per nesting level.
    let ids: Vec<i64> = participants.iter().map(|id| i64::from(*id)).collect();
    let rows = sqlx::query("SELECT item_id,container_id FROM item_ownership WHERE item_id=ANY($1)")
        .bind(&ids)
        .fetch_all(&mut **tx)
        .await?;
    let parents: std::collections::BTreeMap<u32, u32> = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<i64, _>("item_id") as u32,
                row.get::<i64, _>("container_id") as u32,
            )
        })
        .collect();
    let owners: Vec<i64> = sqlx::query_scalar(
        "SELECT owner_id FROM house_ownership WHERE object_id=ANY($1) AND owner_id IS NOT NULL",
    )
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    if owners
        .iter()
        .any(|owner| !participants.contains(&(*owner as u32)))
    {
        return Err(StoreError::Invalid("unreserved housing owner"));
    }
    for start in participants {
        let mut current = *start;
        let mut visited = BTreeSet::from([current]);
        while let Some(parent) = parents.get(&current) {
            if visited.len() >= 64 || !visited.insert(*parent) || !participants.contains(parent) {
                return Err(StoreError::Invalid(
                    "cyclic, excessive or unreserved inventory ancestry",
                ));
            }
            current = *parent;
        }
    }
    Ok(())
}

fn validate(operation: &InventoryOperation) -> Result<BTreeSet<u32>, StoreError> {
    crate::writes::validate(&operation.snapshots)?;
    if operation.operation_id.is_empty()
        || operation.operation_id.len() > 128
        || operation.transfers.len() > 1024
        || operation.leases.len() > 1024
    {
        return Err(StoreError::Invalid("inventory operation bounds"));
    }
    let participants: BTreeSet<_> = operation.snapshots.iter().map(|s| s.object_id).collect();
    let mut leases = BTreeSet::new();
    for lease in &operation.leases {
        if !matches!(
            lease.state,
            OwnershipState::Online | OwnershipState::Offline
        ) || lease.epoch < 0
            || !participants.contains(&lease.character_id)
            || !leases.insert(lease.character_id)
        {
            return Err(StoreError::OwnershipConflict);
        }
    }
    let mut items = BTreeSet::new();
    for transfer in &operation.transfers {
        if !items.insert(transfer.item)
            || !participants.contains(&transfer.item)
            || transfer.expected == transfer.destination
        {
            return Err(StoreError::Invalid(
                "duplicate, absent or unchanged transferred item",
            ));
        }
        for place in [transfer.expected, transfer.destination]
            .into_iter()
            .flatten()
        {
            if place.container == transfer.item || !participants.contains(&place.container) {
                return Err(StoreError::Invalid("unreserved transfer participant"));
            }
        }
    }
    Ok(participants)
}

fn fingerprint(operation: &InventoryOperation) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"betterace-inventory-operation-v1");
    let mut snapshots: Vec<_> = operation.snapshots.iter().collect();
    snapshots.sort_by_key(|s| s.object_id);
    hash.update((snapshots.len() as u64).to_le_bytes());
    for s in snapshots {
        hash.update(s.object_id.to_le_bytes());
        hash.update(s.mutation_revision.to_le_bytes());
        hash.update(s.expected_version.to_le_bytes());
        hash.update((s.bytes.len() as u64).to_le_bytes());
        hash.update(&s.bytes);
    }
    let mut leases: Vec<_> = operation.leases.iter().collect();
    leases.sort_by_key(|l| l.character_id);
    hash.update((leases.len() as u64).to_le_bytes());
    for l in leases {
        hash.update(l.character_id.to_le_bytes());
        hash.update(l.epoch.to_le_bytes());
        hash.update([u8::from(l.state == OwnershipState::Online)]);
    }
    let mut transfers: Vec<_> = operation.transfers.iter().collect();
    transfers.sort_by_key(|t| t.item);
    hash.update((transfers.len() as u64).to_le_bytes());
    for t in transfers {
        hash.update(t.item.to_le_bytes());
        for place in [t.expected, t.destination] {
            hash.update([u8::from(place.is_some())]);
            if let Some(p) = place {
                hash.update(p.container.to_le_bytes());
                hash.update(p.slot.to_le_bytes());
            }
        }
    }
    hash.finalize().into()
}
