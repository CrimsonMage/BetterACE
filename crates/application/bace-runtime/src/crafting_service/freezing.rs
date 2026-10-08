use super::*;
use crate::crafting_saves::{
    self, CraftFreezeInput, CraftingSavedItem, CraftingSavedPlayer, SalvageBagTemplate,
    SalvageFreezeInput,
};
use std::collections::BTreeMap;
pub(super) fn freeze(
    work: &CraftingWork,
    snapshot: &PlayerReadSnapshot,
    unix: u64,
    online: &OnlinePlayerSaveService,
) -> Result<PendingPlacementSave, String> {
    if snapshot.binding() != work.binding {
        return Err("crafting capture binding mismatch".into());
    }
    let (baseline, version, lease) = online
        .baseline(work.binding.actor.0)
        .ok_or("crafting baseline missing")?;
    let saved = crate::player_saves::freeze_player_operation_baseline(
        baseline,
        snapshot,
        PlayerSnapshotOperation::Crafting(work.ticket.operation),
        revision(&work.ticket),
        unix,
    )
    .map_err(|e| e.to_string())?;
    let player = CraftingSavedPlayer {
        saved,
        persisted_version: version,
    };
    let items = online
        .operation_inventory_baselines(snapshot)?
        .into_iter()
        .map(|item| {
            Ok((
                item.entity.object_id,
                CraftingSavedItem {
                    saved: bace_storage_codec::ItemSaveV5 {
                        previous: bace_storage_codec::ItemSaveV4 {
                            previous: bace_storage_codec::ItemSaveV3 {
                                previous: bace_storage_codec::ItemSaveV2 {
                                    entity: item.entity,
                                    placement: item
                                        .placement
                                        .ok_or("crafting item placement missing")?,
                                },
                                enchantments: item.enchantments,
                            },
                            construction: item.construction.clone(),
                        },
                        source_destination: item.source_destination,
                    },
                    persisted_version: item.persisted_version,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let participants: Vec<_> = work
        .ticket
        .inventory
        .participants
        .iter()
        .map(|(id, _)| id.0)
        .collect();
    let mut operation = match &work.ticket.decision {
        CraftingDecision::Tinker(proposal) => {
            let source = items
                .get(&proposal.source_before.id)
                .ok_or("craft source missing")?;
            let target = items
                .get(&proposal.target_before.id)
                .ok_or("craft target missing")?;
            let others = items
                .iter()
                .filter(|(id, _)| {
                    **id != source.saved.entity.object_id && **id != target.saved.entity.object_id
                })
                .map(|(_, s)| CraftingSavedItem {
                    saved: s.saved.clone(),
                    persisted_version: s.persisted_version,
                })
                .collect::<Vec<_>>();
            crafting_saves::freeze_craft(CraftFreezeInput {
                proposal,
                player: &player,
                source,
                target,
                participants: &participants,
                lease,
                inventory: &work.ticket.inventory,
                other_items: &others,
            })
        }
        CraftingDecision::Salvage(proposal) => {
            let consumed = proposal
                .consumed
                .iter()
                .map(|(id, _)| {
                    let item = items.get(id).ok_or("salvage consumed source missing")?;
                    Ok(CraftingSavedItem {
                        saved: item.saved.clone(),
                        persisted_version: item.persisted_version,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let generated = work
                .generated
                .iter()
                .map(|entity| {
                    let change = work
                        .ticket
                        .inventory
                        .changes
                        .iter()
                        .find(|c| c.after.id.0 == entity.object_id)
                        .ok_or("salvage generated identity missing")?;
                    let bace_inventory::ItemPlace::Contained {
                        container,
                        slot,
                        equipped: 0,
                    } = change.after.place
                    else {
                        return Err("salvage generated placement".to_string());
                    };
                    if container != work.binding.actor {
                        return Err("salvage generated owner".into());
                    }
                    Ok(SalvageBagTemplate {
                        entity: entity.clone(),
                        slot,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let others = items
                .iter()
                .filter(|(id, _)| !proposal.consumed.iter().any(|(cid, _)| cid == *id))
                .map(|(_, s)| CraftingSavedItem {
                    saved: s.saved.clone(),
                    persisted_version: s.persisted_version,
                })
                .collect::<Vec<_>>();
            crafting_saves::freeze_salvage(SalvageFreezeInput {
                proposal,
                player: &player,
                consumed: &consumed,
                generated: &generated,
                participants: &participants,
                lease,
                inventory: &work.ticket.inventory,
                other_items: &others,
            })
        }
    }
    .map_err(|e| e.to_string())?;
    operation = crafting_saves::join_proficiency(operation, &player, &work.ticket)
        .map_err(|e| e.to_string())?;
    // Both salvage and unchanged-player tinkering still capture all pre-existing
    // dirty state in this transaction. A routine baseline cannot be marked clean
    // merely because the crafting delta did not change the player's revision.
    if !operation
        .snapshots
        .iter()
        .any(|s| s.object_id == work.binding.actor.0)
        && player.saved != *baseline
    {
        operation.snapshots.push(SaveSnapshot {
            object_id: work.binding.actor.0,
            mutation_revision: player.saved.player.entity.mutation_revision,
            expected_version: version,
            bytes: player.saved.encode().map_err(|e| e.to_string())?,
        });
    }
    let previous = online.inventory_baselines(work.binding.actor.0);
    for item in items.values() {
        let id = item.saved.entity.object_id;
        if operation.snapshots.iter().any(|s| s.object_id == id) {
            continue;
        }
        let old = previous
            .iter()
            .find(|s| s.entity.object_id == id)
            .ok_or("crafting original item missing")?;
        if old.entity != item.saved.entity || old.enchantments != item.saved.enchantments {
            operation.snapshots.push(SaveSnapshot {
                object_id: id,
                mutation_revision: item.saved.entity.mutation_revision,
                expected_version: item.persisted_version,
                bytes: item.saved.encode().map_err(|e| e.to_string())?,
            });
            if !operation.participants.contains(&id) {
                operation.participants.push(id);
            }
        }
    }
    operation.snapshots.sort_by_key(|s| s.object_id);
    operation.participants.sort_unstable();
    PendingPlacementSave::new(operation).map_err(|e| e.to_string())
}
