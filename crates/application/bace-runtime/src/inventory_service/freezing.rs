//! Freeze the accepted before-state and complete new placements as one operation.
use super::*;
use crate::game_inventory::{InventoryFreezeInput, freeze_generated_inventory, freeze_inventory};
use bace_content::Position;
use bace_storage_codec::{ItemPlacementV2, ItemSaveV5};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn freeze(
    work: &InventoryWork,
    snapshot: &PlayerReadSnapshot,
    unix: u64,
    online: &OnlinePlayerSaveService,
) -> Result<PendingPlacementSave, String> {
    let p = &work.operation;
    if snapshot.binding() != work.binding {
        return Err("inventory snapshot binding mismatch".into());
    }
    let (baseline, version, lease) = online
        .baseline(work.binding.actor.0)
        .ok_or("inventory player baseline missing")?;
    let mut player = crate::player_saves::freeze_player_operation_baseline(
        baseline,
        snapshot,
        PlayerSnapshotOperation::Inventory(p.ticket.operation),
        p.actor_revision,
        unix,
    )
    .map_err(|e| e.to_string())?;
    let mut items = online.operation_inventory_baselines(snapshot)?;
    for source in &work.external {
        if items
            .iter()
            .any(|i| i.entity.object_id == source.entity.object_id)
        {
            return Err("duplicate owned/external inventory baseline".into());
        }
        items.push(source.clone());
    }
    if let Some(fresh) = &work.fresh {
        if items
            .iter()
            .any(|i| i.entity.object_id == fresh.frozen.entity.object_id)
        {
            return Err("fresh split already has a baseline".into());
        }
        items.push(fresh.frozen.clone());
    }
    let positions: BTreeMap<_, _> = p
        .positions
        .iter()
        .map(|(id, p)| {
            (
                id.0,
                Position {
                    obj_cell_id: p.cell,
                    position_x: p.origin[0],
                    position_y: p.origin[1],
                    position_z: p.origin[2],
                    rotation_w: p.rotation[3],
                    rotation_x: p.rotation[0],
                    rotation_y: p.rotation[1],
                    rotation_z: p.rotation[2],
                },
            )
        })
        .collect();
    for change in &p.ticket.proposal.changes {
        let source = items
            .iter_mut()
            .find(|i| i.entity.object_id == change.after.id.0)
            .ok_or("inventory participant baseline missing")?;
        if let Some(before) = &change.before {
            // Only immutable registry/placement/stack projections are overlaid.
            // Instance property mutations must already be present in this source.
            if source.entity.mutation_revision > before.revision
                || source.entity.state.weenie_id != before.template
            {
                return Err("inventory baseline fence mismatch".into());
            }
            source.entity.mutation_revision = before.revision;
            source.placement = Some(match before.place {
                bace_inventory::ItemPlace::World => ItemPlacementV2::World(
                    positions
                        .get(&before.id.0)
                        .ok_or("inventory accepted before pose missing")?
                        .clone(),
                ),
                bace_inventory::ItemPlace::Contained {
                    container,
                    slot,
                    equipped,
                } => ItemPlacementV2::Contained {
                    container: container.0,
                    slot,
                    pack_slot: before.pack_slot,
                    equipped,
                },
                bace_inventory::ItemPlace::Removed => {
                    return Err("inventory participant already removed".into());
                }
            });
            crate::game_inventory::set(
                &mut source.entity.state.properties.ints,
                12,
                i32::try_from(before.stack).map_err(|_| "inventory stack overflow")?,
            );
            match before.structure {
                Some(n) => crate::game_inventory::set(
                    &mut source.entity.state.properties.ints,
                    92,
                    i32::try_from(n).map_err(|_| "inventory structure overflow")?,
                ),
                None => source.entity.state.properties.ints.retain(|p| p.id != 92),
            }
            if let Some(entries) = p.enchantments.get(&before.id) {
                source.enchantments = entries
                    .iter()
                    .map(crate::enchantment_saves::freeze_enchantment)
                    .collect::<Result<_, _>>()
                    .map_err(|e| e.to_string())?;
            } else if !source.enchantments.is_empty() {
                return Err("inventory held registry missing".into());
            }
        }
    }
    if let Some(patch) = &p.equipment {
        let vitals = p
            .equipment_vitals
            .as_ref()
            .ok_or("equipment physical companion missing")?;
        if vitals.after_revision
            != p.actor_revision
                .checked_add(1)
                .ok_or("equipment revision overflow")?
        {
            return Err("equipment actor revision".into());
        }
        crate::equipment_effects::overlay_equipment_player_registry(&mut player, patch)
            .map_err(|e| e.to_string())?;
        player.player.entity.mutation_revision = vitals.after_revision;
        if let Some(health) = vitals.gear_health {
            player
                .player
                .entity
                .state
                .properties
                .ints
                .retain(|p| p.id != 379);
            if health != 0 {
                player
                    .player
                    .entity
                    .state
                    .properties
                    .ints
                    .push(bace_content::Property {
                        id: 379,
                        value: i32::try_from(health).map_err(|_| "equipment health overflow")?,
                    });
            }
        }
        for (index, key) in [1u16, 3, 5].into_iter().enumerate() {
            let vital = player
                .player
                .entity
                .state
                .properties
                .secondary_attributes
                .iter_mut()
                .find(|p| p.id == u32::from(key))
                .ok_or("equipment saved vital missing")?;
            if vital.value.current_level != vitals.before[index] {
                return Err("equipment vital before-image".into());
            }
            vital.value.current_level = vitals.after[index];
        }
    }
    let mut extra = online.operation_inventory_changes(snapshot)?;
    extra.retain(|row| {
        !p.ticket
            .proposal
            .changes
            .iter()
            .any(|c| c.after.id.0 == row.object_id)
    });
    if player != *baseline {
        extra.push(SaveSnapshot {
            object_id: work.binding.actor.0,
            mutation_revision: player.player.entity.mutation_revision,
            expected_version: version,
            bytes: player.encode().map_err(|e| e.to_string())?,
        });
    }
    let operation_id = format!(
        "inventory:{}:{}:{}",
        work.world_epoch, work.binding.actor.0, p.ticket.operation
    );
    let transient: BTreeSet<_> = p.transient.iter().map(|id| id.0).collect();
    let mut creature_roots: Vec<_> = items
        .iter()
        .filter(|item| {
            transient.contains(&item.entity.object_id)
                && item.construction.is_some()
                && matches!(item.entity.state.weenie_type, 10 | 15)
        })
        .map(|item| item.entity.object_id)
        .collect();
    creature_roots.sort_unstable();
    creature_roots.dedup();
    let input = InventoryFreezeInput {
        operation_id: &operation_id,
        proposal: &p.ticket.proposal,
        items: &items,
        other_snapshots: &extra,
        leases: &[lease],
        storage_views: &work.storage_views,
        admitted_positions: &positions,
    };
    let mut operation = if let Some(patch) = &p.equipment {
        if !p.transient.is_empty() {
            return Err("equipment transient participant unsupported".into());
        }
        bace_persistence::WorldPlacementOperation {
            world_epoch: work.world_epoch,
            inventory: crate::equipment_effects::freeze_equipment_inventory(input, patch)
                .map_err(|e| e.to_string())?,
        }
    } else if p.transient.is_empty() {
        bace_persistence::WorldPlacementOperation {
            world_epoch: work.world_epoch,
            inventory: freeze_inventory(input).map_err(|e| e.to_string())?,
        }
    } else {
        freeze_generated_inventory(
            input,
            &p.transient.iter().map(|id| id.0).collect::<Vec<_>>(),
            work.world_epoch,
        )
        .map_err(|e| e.to_string())?
    };
    super::corpse::freeze(
        &mut operation.inventory,
        &items,
        &p.corpse_decay,
        snapshot.tick(),
        unix,
    )?;
    let size = operation
        .inventory
        .snapshots
        .iter()
        .try_fold(0usize, |sum, row| sum.checked_add(row.bytes.len()))
        .ok_or("inventory frozen bytes overflow")?;
    if size > 64 * 1024 * 1024 {
        return Err("inventory frozen byte capacity".into());
    }
    // New stack rows are full ItemSaveV5 factory instances with an explicit final
    // placement. An absent creation description can never count as a successful split.
    if let Some(fresh) = &work.fresh {
        let row = operation
            .inventory
            .snapshots
            .iter()
            .find(|s| s.object_id == fresh.item.id.0)
            .ok_or("fresh split row missing")?;
        let saved = ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
        if matches!(saved.placement, ItemPlacementV2::Removed)
            || saved.entity.state.weenie_id != fresh.item.template
        {
            return Err("fresh split incomplete state".into());
        }
    }
    // A first durable Creature/Cow graph cannot enter the ordinary world
    // placement lane: that lane intentionally rejects fresh construction.
    if creature_roots.is_empty() {
        PendingPlacementSave::new_world(operation).map_err(|e| e.to_string())
    } else {
        PendingPlacementSave::new_constructed(
            bace_persistence::ConstructedCreaturePromotionOperation {
                world_epoch: operation.world_epoch,
                inventory: operation.inventory,
                creature_roots,
            },
        )
        .map_err(|e| e.to_string())
    }
}
