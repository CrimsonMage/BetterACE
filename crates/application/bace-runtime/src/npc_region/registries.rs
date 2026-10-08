//! Restored registries are installed during owner admission, before initial
//! item-spell callbacks or AI can run. The cold DAT definitions remain pinned.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn restore_object_registry(
    location: bace_simulation::NpcSourceLocation,
    prepared: &crate::region_activation::PreparedRegionActivation,
    proof: Option<&bace_storage_codec::npc_workflow_v3::NpcSourceInventoryV3>,
) -> Result<Option<bace_simulation::PreparedNpcRegistryRestore>, String> {
    if location.facts.creature {
        return Ok(None);
    }
    let Some(proof) = proof.filter(|p| p.source_registry_revision.is_some()) else {
        return Ok(None);
    };
    let actor = prepared
        .content
        .instances
        .first()
        .ok_or("NPC restored object source missing")?
        .source
        .guid;
    let spells = prepared.item_spells.as_ref().map_err(Clone::clone)?;
    let sets: BTreeSet<_> = spells
        .table
        .sets
        .values()
        .flat_map(|tiers| tiers.values().flatten().copied())
        .collect();
    let entries = proof
        .source_enchantments
        .iter()
        .map(|e| {
            crate::enchantment_saves::restore_enchantment(
                e,
                crate::region_service::world_items::definition(
                    &spells.table,
                    &sets,
                    e.spell_id as u32,
                ),
            )
            .map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    Ok(Some(bace_simulation::PreparedNpcRegistryRestore {
        entity: bace_types::EntityId(actor),
        revision: 0,
        entries,
    }))
}
pub(crate) fn restore_loadout(
    pin: &PreparedNpcRegionSource,
    loadout: &mut bace_simulation::PreparedNpcLoadout,
    sources: &BTreeMap<u32, crate::region_unload_saves::RegionItemSource>,
) -> Result<(), String> {
    let proof = pin
        .inventory
        .as_ref()
        .ok_or("NPC inventory proof missing")?;
    let spells = pin.prepared.item_spells.as_ref().map_err(Clone::clone)?;
    let sets: BTreeSet<_> = spells
        .table
        .sets
        .values()
        .flat_map(|tiers| tiers.values().flatten().copied())
        .collect();
    let entries = |saved: &[bace_storage_codec::FrozenEnchantmentV1]| {
        saved
            .iter()
            .map(|e| {
                crate::enchantment_saves::restore_enchantment(
                    e,
                    crate::region_service::world_items::definition(
                        &spells.table,
                        &sets,
                        e.spell_id as u32,
                    ),
                )
                .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, String>>()
    };
    let mut registries = vec![bace_simulation::PreparedNpcRegistryRestore {
        entity: pin.registration.actor,
        revision: 0,
        entries: entries(&proof.source_enchantments)?,
    }];
    for item in &loadout.items {
        let source = sources
            .get(&item.id.0)
            .ok_or("NPC restored gear source missing")?;
        // Registry revisions are in-memory CAS counters. The new world owner
        // starts them at zero; SQL version and item mutation revision remain
        // the durable custody/image fences, including later accepted changes.
        registries.push(bace_simulation::PreparedNpcRegistryRestore {
            entity: item.id,
            revision: 0,
            entries: entries(&source.item.enchantments)?,
        });
    }
    let assets = loadout
        .combat_assets
        .as_mut()
        .ok_or("NPC restored combat source missing")?;
    Arc::make_mut(assets).restored_registries = Some(registries);
    loadout.enchantments.clear();
    let ids: Vec<_> = proof
        .death_items
        .iter()
        .copied()
        .filter(|id| loadout.items.iter().any(|i| i.id.0 == *id))
        .collect();
    let mut parents = Vec::new();
    let mut rows = Vec::new();
    for (index, &id) in ids.iter().enumerate() {
        let item = loadout
            .items
            .iter()
            .find(|i| i.id.0 == id)
            .ok_or("NPC death gear identity")?;
        let parent = match item.place {
            bace_inventory::ItemPlace::Contained { container, .. } => {
                ids.iter().position(|id| *id == container.0)
            }
            _ => return Err("NPC death gear custody".into()),
        };
        if parent.is_some_and(|p| p >= index) {
            return Err("NPC death gear parent chronology".into());
        }
        parents.push(parent);
        rows.push(
            sources
                .get(&id)
                .ok_or("NPC death gear source")?
                .item
                .entity
                .state
                .clone(),
        );
    }
    loadout.death_ids = ids.into_iter().map(bace_types::EntityId).collect();
    loadout.death_items = rows;
    loadout.death_parents = parents;
    Ok(())
}
