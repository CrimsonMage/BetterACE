//! Decode the exact committed corpse forest before publishing death completion.
//! The durable snapshots, not mutable player inventory, own the source rows.
use crate::{
    game_inventory::FrozenInventoryItem, player_death_service::PlayerDeathCompletion,
    region_unload_saves::RegionItemSource,
};
use bace_storage_codec::ItemPlacementV2;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn committed(
    completion: &PlayerDeathCompletion,
) -> Result<Vec<RegionItemSource>, String> {
    let corpse_id = completion.work.ticket.corpse.0;
    let mut rows = BTreeMap::new();
    if completion.committed.len() > 1024 {
        return Err("death committed source count".into());
    }
    for snapshot in &completion.committed {
        let header = bace_storage_codec::inspect(
            &snapshot.bytes,
            bace_storage_codec::CodecLimits {
                max_payload_bytes: 2 * 1024 * 1024,
            },
        )
        .map_err(|e| e.to_string())?;
        if !matches!(header.kind, 101 | 102) {
            continue;
        }
        let version = snapshot
            .expected_version
            .checked_add(1)
            .ok_or("death committed source version overflow")?;
        let item = FrozenInventoryItem::decode(&snapshot.bytes, None, version)
            .map_err(|e| e.to_string())?;
        if item.entity.object_id != snapshot.object_id
            || item.entity.mutation_revision != snapshot.mutation_revision
            || rows.insert(snapshot.object_id, item).is_some()
        {
            return Err("death committed source identity".into());
        }
    }
    let root = rows
        .remove(&corpse_id)
        .ok_or("death committed corpse absent")?;
    let corpse = root
        .corpse
        .as_ref()
        .ok_or("death committed corpse descriptor absent")?;
    if corpse.source != Some(completion.work.binding.actor.0)
        || corpse.operation != Some(completion.work.ticket.operation)
    {
        return Err("death committed corpse operation mismatch".into());
    }
    let mut out = vec![RegionItemSource {
        corpse: root.corpse.as_ref().map(|c| (**c).clone()),
        item: root,
    }];
    let mut included = BTreeSet::from([corpse_id]);
    let mut index = 0;
    while index < out.len() {
        let parent = out[index].item.entity.object_id;
        let mut children = rows
            .iter()
            .filter_map(|(&id, item)| match &item.placement {
                Some(ItemPlacementV2::Contained {
                    container,
                    slot,
                    pack_slot,
                    equipped: 0,
                }) if *container == parent => Some((*pack_slot, *slot, id)),
                _ => None,
            })
            .collect::<Vec<_>>();
        children.sort_unstable();
        for (_, _, id) in children {
            let item = rows.remove(&id).ok_or("death child source disappeared")?;
            if !included.insert(id) || out.len() >= 1024 {
                return Err("death corpse forest duplicate/capacity".into());
            }
            out.push(RegionItemSource { item, corpse: None });
        }
        index += 1;
    }
    if completion
        .work
        .ticket
        .corpse_items
        .iter()
        .any(|id| !included.contains(&id.0))
    {
        return Err("death corpse item source missing".into());
    }
    Ok(out)
}

/// The distinct Bool29 world forest comes from the committed item rows. A
/// player checkpoint can commit with zero roots, so an empty forest is valid.
pub(super) fn committed_world_drops(
    completion: &PlayerDeathCompletion,
) -> Result<Vec<RegionItemSource>, String> {
    let plan = completion
        .work
        .ticket
        .no_corpse
        .as_ref()
        .ok_or("NoCorpse plan absent")?;
    committed_world_drop_rows(
        plan,
        &completion.work.ticket.inventory.proposal.changes,
        &completion.committed,
    )
}
fn committed_world_drop_rows(
    plan: &bace_simulation::PlayerNoCorpsePlan,
    changes: &[bace_inventory::ItemChange],
    committed: &[bace_persistence::SaveSnapshot],
) -> Result<Vec<RegionItemSource>, String> {
    if committed.len() > 1024 || plan.world_roots.len() > 1024 {
        return Err("NoCorpse committed source count".into());
    }
    let mut rows = BTreeMap::new();
    for snapshot in committed {
        let header = bace_storage_codec::inspect(
            &snapshot.bytes,
            bace_storage_codec::CodecLimits {
                max_payload_bytes: 2 * 1024 * 1024,
            },
        )
        .map_err(|e| e.to_string())?;
        if header.kind != 101 {
            continue;
        }
        let item = FrozenInventoryItem::decode(&snapshot.bytes, None, snapshot.expected_version)
            .map_err(|e| e.to_string())?;
        if item.entity.object_id != snapshot.object_id
            || item.entity.mutation_revision != snapshot.mutation_revision
            || rows.insert(snapshot.object_id, item).is_some()
        {
            return Err("NoCorpse committed item identity".into());
        }
    }
    let mut out = Vec::new();
    let mut included = BTreeSet::new();
    for id in &plan.world_roots {
        let root = rows.remove(&id.0).ok_or("NoCorpse committed root absent")?;
        if root.placement != Some(ItemPlacementV2::World(plan.accepted_position.clone()))
            || !included.insert(id.0)
        {
            return Err("NoCorpse committed root pose/identity".into());
        }
        out.push(RegionItemSource {
            item: root,
            corpse: None,
        });
    }
    let mut index = 0;
    while index < out.len() {
        let parent = out[index].item.entity.object_id;
        let mut children = rows
            .iter()
            .filter_map(|(&id, item)| match &item.placement {
                Some(ItemPlacementV2::Contained {
                    container,
                    slot,
                    pack_slot,
                    equipped: 0,
                }) if *container == parent => Some((*pack_slot, *slot, id)),
                _ => None,
            })
            .collect::<Vec<_>>();
        children.sort_unstable();
        for (_, _, id) in children {
            let item = rows.remove(&id).ok_or("NoCorpse descendant disappeared")?;
            if !included.insert(id) || out.len() >= 1024 {
                return Err("NoCorpse committed forest duplicate/capacity".into());
            }
            out.push(RegionItemSource { item, corpse: None });
        }
        index += 1;
    }
    let selected: BTreeSet<_> = changes
        .iter()
        .filter(|change| match change.after.place {
            bace_inventory::ItemPlace::World => true,
            bace_inventory::ItemPlace::Contained { container, .. } => {
                included.contains(&container.0)
            }
            bace_inventory::ItemPlace::Removed => false,
        })
        .map(|change| change.after.id.0)
        .collect();
    for descendant in &plan.descendants {
        let change = changes
            .iter()
            .find(|change| change.after.id == descendant.id)
            .ok_or("NoCorpse descendant change absent")?;
        let before = change
            .before
            .as_ref()
            .ok_or("NoCorpse descendant source absent")?;
        if !selected.contains(&descendant.id.0)
            || before.place != change.after.place
            || before.revision.checked_add(1) != Some(descendant.revision)
            || change.after.revision != descendant.revision
        {
            return Err("NoCorpse descendant receipt change mismatch".into());
        }
        let source = out
            .iter()
            .find(|source| source.item.entity.object_id == descendant.id.0)
            .ok_or("NoCorpse committed descendant absent")?;
        if source.item.entity.mutation_revision != descendant.revision
            || source.item.placement
                != Some(ItemPlacementV2::Contained {
                    container: descendant.parent.0,
                    slot: descendant.slot,
                    pack_slot: descendant.pack_slot,
                    equipped: 0,
                })
        {
            return Err("NoCorpse committed descendant placement/revision".into());
        }
    }
    if selected != included {
        return Err("NoCorpse committed forest incomplete".into());
    }
    Ok(out)
}

/// Reconstruct only the world roots and their contained descendants from the
/// exact corpse-expiry placement receipt. The corpse tombstone is excluded.
pub(super) fn committed_spill(
    ticket: &bace_simulation::CorpseExpiryTicket,
    presentation: &super::CorpseSpillPresentation,
) -> Result<Vec<RegionItemSource>, String> {
    let Some(intent) = ticket.spill.as_ref() else {
        return Err("spill receipt lacks intent".into());
    };
    if presentation.snapshots.len() > 1024 || intent.roots.len() > 1024 {
        return Err("spill committed source count".into());
    }
    let mut rows = BTreeMap::new();
    for snapshot in &presentation.snapshots {
        let header = bace_storage_codec::inspect(
            &snapshot.bytes,
            bace_storage_codec::CodecLimits {
                max_payload_bytes: 2 * 1024 * 1024,
            },
        )
        .map_err(|e| e.to_string())?;
        if header.kind != 101 {
            continue;
        }
        let version = snapshot
            .expected_version
            .checked_add(1)
            .ok_or("spill committed version overflow")?;
        let item = FrozenInventoryItem::decode(&snapshot.bytes, None, version)
            .map_err(|e| e.to_string())?;
        if item.entity.object_id != snapshot.object_id
            || item.entity.mutation_revision != snapshot.mutation_revision
            || rows.insert(snapshot.object_id, item).is_some()
        {
            return Err("spill committed source identity".into());
        }
    }
    let mut out = Vec::new();
    let mut included = BTreeSet::new();
    for root in &intent.roots {
        let item = rows.remove(&root.0).ok_or("spill root source absent")?;
        if !matches!(item.placement, Some(ItemPlacementV2::World(_))) || !included.insert(root.0) {
            return Err("spill root placement/identity".into());
        }
        out.push(RegionItemSource { item, corpse: None });
    }
    let mut index = 0;
    while index < out.len() {
        let parent = out[index].item.entity.object_id;
        let mut children = rows
            .iter()
            .filter_map(|(&id, item)| match &item.placement {
                Some(ItemPlacementV2::Contained {
                    container,
                    slot,
                    pack_slot,
                    equipped: 0,
                }) if *container == parent => Some((*pack_slot, *slot, id)),
                _ => None,
            })
            .collect::<Vec<_>>();
        children.sort_unstable();
        for (_, _, id) in children {
            let item = rows.remove(&id).ok_or("spill descendant disappeared")?;
            if !included.insert(id) || out.len() >= 1024 {
                return Err("spill descendant duplicate/capacity".into());
            }
            out.push(RegionItemSource { item, corpse: None });
        }
        index += 1;
    }
    if ticket.inventory.proposal.changes.iter().any(|change| {
        change.after.place != bace_inventory::ItemPlace::Removed
            && !included.contains(&change.after.id.0)
    }) {
        return Err("spill changed source absent".into());
    }
    Ok(out)
}

#[cfg(test)]
mod no_corpse_tests {
    use super::*;
    use bace_inventory::{InventoryItem, ItemChange, ItemPlace};
    use bace_storage_codec::{EntitySaveV1, ItemSaveV2, ItemSaveV4, ItemSaveV5};
    use bace_types::EntityId;

    #[test]
    fn committed_selected_container_admits_only_exact_nested_v5_forest() {
        let actor = EntityId(0x50000001);
        let root = EntityId(0x80000011);
        let child = EntityId(0x80000012);
        let position = bace_content::Position {
            obj_cell_id: 0x12340001,
            position_x: 1.,
            position_y: 2.,
            position_z: 3.,
            rotation_w: 1.,
            ..Default::default()
        };
        let plan = bace_simulation::PlayerNoCorpsePlan {
            world_roots: vec![root],
            descendants: vec![bace_simulation::NoCorpseDescendant {
                id: child,
                parent: root,
                slot: 2,
                pack_slot: true,
                revision: 2,
            }],
            accepted_position: position.clone(),
        };
        let item = |id, container, slot, pack_slot, is_container| InventoryItem {
            id,
            revision: 1,
            template: id.0,
            stack_key: 1,
            place: ItemPlace::Contained {
                container,
                slot,
                equipped: 0,
            },
            stack: 1,
            maximum_stack: 1,
            unit_burden: 1,
            unit_value: 1,
            pack_slot,
            is_container,
            attuned: false,
            trade_reserved: false,
            active_pet: false,
            unique: false,
            quest_allowed: true,
            valid_wield: 0,
            incompatible_wield: 0,
            wield_requirements_met: false,
            structure: None,
        };
        let root_before = item(root, actor, 0, true, true);
        let mut root_after = root_before.clone();
        root_after.place = ItemPlace::World;
        root_after.revision = 2;
        let child_before = item(child, root, 2, true, false);
        let mut child_after = child_before.clone();
        child_after.revision = 2;
        let changes = [
            ItemChange {
                before: Some(root_before),
                after: root_after,
            },
            ItemChange {
                before: Some(child_before),
                after: child_after,
            },
        ];
        let saved = |id: EntityId, kind: u32, origin: u8, placement: ItemPlacementV2| ItemSaveV5 {
            source_destination: Some(origin),
            previous: ItemSaveV4::migrate_v2(ItemSaveV2 {
                entity: EntitySaveV1 {
                    object_id: id.0,
                    template_revision: 1,
                    mutation_revision: 2,
                    state: bace_content::WeenieV1 {
                        schema_version: 1,
                        weenie_id: id.0,
                        class_name: format!("no_corpse_{id:?}"),
                        weenie_type: kind,
                        last_modified: None,
                        properties: Default::default(),
                    },
                },
                placement,
            })
            .unwrap(),
        };
        let root_saved = saved(root, 20, 8, ItemPlacementV2::World(position));
        let mut child_saved = saved(
            child,
            1,
            1,
            ItemPlacementV2::Contained {
                container: root.0,
                slot: 2,
                pack_slot: true,
                equipped: 0,
            },
        );
        let row = |value: &ItemSaveV5| bace_persistence::SaveSnapshot {
            object_id: value.entity.object_id,
            mutation_revision: value.entity.mutation_revision,
            expected_version: 2,
            bytes: value.encode().unwrap(),
        };
        let rows = [row(&root_saved), row(&child_saved)];
        let out = committed_world_drop_rows(&plan, &changes, &rows).unwrap();
        assert_eq!(
            out.iter()
                .map(|row| row.item.entity.object_id)
                .collect::<Vec<_>>(),
            vec![root.0, child.0]
        );
        assert_eq!(out[1].item.placement, Some(child_saved.placement.clone()));
        assert!(committed_world_drop_rows(&plan, &changes, &rows[..1]).is_err());
        child_saved.placement = ItemPlacementV2::Contained {
            container: root.0,
            slot: 3,
            pack_slot: true,
            equipped: 0,
        };
        assert!(
            committed_world_drop_rows(&plan, &changes, &[row(&root_saved), row(&child_saved)])
                .is_err()
        );
    }
}
