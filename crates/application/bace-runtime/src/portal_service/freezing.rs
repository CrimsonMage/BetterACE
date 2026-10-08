use super::*;
use bace_storage_codec::{ItemSaveV2, ItemSaveV5};
pub(super) fn freeze(
    work: &PortalWork,
    captures: &[(Arc<PlayerReadSnapshot>, u64)],
    online: &OnlinePlayerSaveService,
) -> Result<PendingPlacementSave, String> {
    if captures.len() != work.bindings.len() {
        return Err("incomplete portal captures".into());
    }
    let mut players = Vec::with_capacity(captures.len());
    for ((snapshot, unix), binding) in captures.iter().zip(&work.bindings) {
        if snapshot.binding() != *binding {
            return Err("portal capture binding".into());
        }
        let (baseline, version, lease) = online
            .baseline(binding.actor.0)
            .ok_or("portal baseline missing")?;
        let before = work
            .ticket
            .participants
            .iter()
            .find(|(id, _, _)| *id == binding.actor)
            .ok_or("portal participant missing")?
            .1;
        let saved = crate::player_saves::freeze_player_operation_baseline(
            baseline,
            snapshot,
            PlayerSnapshotOperation::Portal(work.ticket.operation),
            before,
            *unix,
        )
        .map_err(|e| e.to_string())?;
        players.push((saved, version, lease));
    }
    let inputs: Vec<_> = players
        .iter()
        .map(
            |(saved, version, lease)| crate::portal_saves::PortalSavePlayer {
                saved,
                version: *version,
                lease: *lease,
            },
        )
        .collect();
    let mut operation =
        crate::portal_saves::freeze_portal_service(work.epoch, &work.ticket, &inputs)
            .map_err(|e| e.to_string())?
            .operation;
    // Preserve unrelated dirty item enchantments in the same aggregate barrier.
    for (snapshot, _) in captures {
        let previous = online.inventory_baselines(snapshot.binding().actor.0);
        for item in online.operation_inventory_baselines(snapshot)? {
            let old = previous
                .iter()
                .find(|old| old.entity.object_id == item.entity.object_id)
                .ok_or("portal item baseline missing")?;
            if old.entity == item.entity && old.enchantments == item.enchantments {
                continue;
            }
            let id = item.entity.object_id;
            let saved_version = item.persisted_version;
            let saved = freeze_portal_item(item)?;
            operation.snapshots.push(SaveSnapshot {
                object_id: id,
                mutation_revision: saved.entity.mutation_revision,
                expected_version: saved_version,
                bytes: saved.encode().map_err(|e| e.to_string())?,
            });
            operation.participants.push(id);
        }
    }
    operation.snapshots.sort_by_key(|s| s.object_id);
    operation.participants.sort_unstable();
    operation.participants.dedup();
    PendingPlacementSave::new(operation).map_err(|e| e.to_string())
}

pub(super) fn freeze_portal_item(
    item: crate::game_inventory::FrozenInventoryItem,
) -> Result<ItemSaveV5, String> {
    Ok(ItemSaveV5 {
        previous: bace_storage_codec::ItemSaveV4 {
            previous: bace_storage_codec::ItemSaveV3 {
                previous: ItemSaveV2 {
                    entity: item.entity,
                    placement: item.placement.ok_or("portal item placement missing")?,
                },
                enchantments: item.enchantments,
            },
            construction: item.construction,
        },
        source_destination: item.source_destination,
    })
}
