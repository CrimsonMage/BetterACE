//! Atomic cold composition of already frozen reward item operations. Duplicate
//! player snapshots are accepted only when every revision and byte is identical.
use bace_persistence::{PlacementOperation, SaveSnapshot};
use bace_storage_codec::SaveCodecError;
pub fn join_reward_item_operations(
    base: &mut PlacementOperation,
    item_operations: Vec<PlacementOperation>,
) -> Result<(), SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("reward item composite mismatch/capacity");
    if base.operation_id.is_empty() || base.operation_id.len() > 128 || item_operations.len() > 64 {
        return Err(invalid());
    }
    let mut next = base.clone();
    // Normalize/reject duplicates in the base too, so no bad prefix survives.
    next.snapshots.clear();
    next.participants.clear();
    next.leases.clear();
    next.changes.clear();
    next.storage_views.clear();
    for operation in std::iter::once(base.clone()).chain(item_operations) {
        if operation.operation_id != base.operation_id
            || operation.snapshots.len() > 1024
            || operation.participants.len() > 4096
            || operation.leases.len() > 1024
            || operation.changes.len() > 1024
            || operation.storage_views.len() > 1024
        {
            return Err(invalid());
        }
        for value in operation.snapshots {
            if let Some(old) = next
                .snapshots
                .iter()
                .find(|old| old.object_id == value.object_id)
            {
                if !same_snapshot(old, &value) {
                    return Err(invalid());
                }
            } else {
                next.snapshots.push(value);
            }
        }
        for value in operation.participants {
            if !next.participants.contains(&value) {
                next.participants.push(value);
            }
        }
        for value in operation.leases {
            if let Some(old) = next
                .leases
                .iter()
                .find(|old| old.character_id == value.character_id)
            {
                if old != &value {
                    return Err(invalid());
                }
            } else {
                next.leases.push(value);
            }
        }
        for value in operation.changes {
            if let Some(old) = next.changes.iter().find(|old| old.item == value.item) {
                if old != &value {
                    return Err(invalid());
                }
            } else {
                next.changes.push(value);
            }
        }
        for value in operation.storage_views {
            if let Some(old) = next
                .storage_views
                .iter()
                .find(|old| old.actor == value.actor && old.house == value.house)
            {
                if old != &value {
                    return Err(invalid());
                }
            } else {
                next.storage_views.push(value);
            }
        }
        if next.snapshots.len() > 1024
            || next.participants.len() > 4096
            || next.leases.len() > 1024
            || next.changes.len() > 1024
            || next.storage_views.len() > 1024
        {
            return Err(invalid());
        }
    }
    next.snapshots.sort_by_key(|s| s.object_id);
    next.participants.sort_unstable();
    next.leases.sort_by_key(|s| s.character_id);
    next.changes.sort_by_key(|s| s.item);
    next.storage_views.sort_by_key(|s| (s.actor, s.house));
    *base = next;
    Ok(())
}
fn same_snapshot(a: &SaveSnapshot, b: &SaveSnapshot) -> bool {
    a.object_id == b.object_id
        && a.mutation_revision == b.mutation_revision
        && a.expected_version == b.expected_version
        && a.bytes == b.bytes
}
/// Correlate each frozen item component to the exact live inventory reservation.
pub fn join_allegiance_item_operations(
    mut base: PlacementOperation,
    ticket: &bace_simulation::AllegianceTicket,
    items: &[(u64, PlacementOperation)],
) -> Result<PlacementOperation, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("reward item operation correlation");
    let expected: Vec<_> = ticket
        .item_experience
        .iter()
        .filter_map(|(_, inventory)| inventory.as_ref())
        .collect();
    if items.len() != expected.len() || items.len() > 64 {
        return Err(invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for (operation, frozen) in items {
        if !seen.insert(*operation) {
            return Err(invalid());
        }
        let live = expected
            .iter()
            .find(|live| live.operation == *operation)
            .ok_or_else(invalid)?;
        if frozen.changes.len() != live.proposal.changes.len()
            || live
                .proposal
                .participants
                .iter()
                .any(|(id, _)| !frozen.participants.contains(&id.0))
        {
            return Err(invalid());
        }
        for change in &live.proposal.changes {
            let before = change.before.as_ref().ok_or_else(invalid)?;
            let expected = bace_persistence::PlacementChange {
                item: change.after.id.0,
                expected: Some(contained(before)?),
                destination: contained(&change.after)?,
            };
            if !frozen.changes.contains(&expected)
                || !frozen.snapshots.iter().any(|s| {
                    s.object_id == change.after.id.0 && s.mutation_revision == change.after.revision
                })
            {
                return Err(invalid());
            }
        }
    }
    join_reward_item_operations(&mut base, items.iter().map(|(_, op)| op.clone()).collect())?;
    Ok(base)
}
fn contained(
    item: &bace_inventory::InventoryItem,
) -> Result<bace_persistence::DurableItemPlace, SaveCodecError> {
    if let bace_inventory::ItemPlace::Contained {
        container,
        slot,
        equipped,
    } = item.place
    {
        Ok(bace_persistence::DurableItemPlace::Contained {
            container: container.0,
            slot,
            pack_slot: item.pack_slot,
            equipped,
        })
    } else {
        Err(SaveCodecError::Invalid("item XP must remain equipped"))
    }
}
