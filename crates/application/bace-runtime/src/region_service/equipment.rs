//! Cold static/encounter equipment uses one immutable activation identity per root.
mod recovery;
use crate::region_activation::PreparedRegionActivation;
use bace_gameplay_api::{
    GeneratorDestination, GeneratorLocation, GeneratorProfile, GeneratorSpawnIntent,
    GeneratorSpawnKey,
};
use bace_types::EntityId;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
pub(super) struct RootEquipment {
    restored: Option<Vec<crate::game_inventory::FrozenInventoryItem>>,
    actor: EntityId,
    source: Arc<bace_content::WeenieV1>,
    items: Vec<bace_loot::PreparedCreatureEquipment>,
}
pub(super) struct RegionEquipment {
    roots: Vec<RootEquipment>,
    pub count: usize,
}
pub(super) fn materialize(
    prepared: &PreparedRegionActivation,
    encounters: &BTreeMap<u32, EntityId>,
    random: &bace_random::RandomRoot,
    world_epoch: u64,
    drop_plain_wield: bool,
) -> Result<RegionEquipment, String> {
    let roots = prepared
        .content
        .instances
        .iter()
        .filter(|i| !i.source.is_link_child)
        .map(|i| (EntityId(i.source.guid), i.template.clone()))
        .chain(
            prepared
                .content
                .encounters
                .iter()
                .map(|e| (encounters[&e.source.id], e.template.clone())),
        );
    let mut result = RegionEquipment {
        roots: vec![],
        count: 0,
    };
    for (actor, source) in roots {
        let pinned = prepared.npc_recovery.get(&actor);
        let prepared = prepared
            .npc_recovery
            .get(&actor)
            .map_or(prepared, |pin| pin.prepared.as_ref());
        let Some(creature) = prepared.creatures.get(&source.weenie_id) else {
            continue;
        };
        let creature = creature.as_ref().map_err(Clone::clone)?;
        if let Some(pinned) = pinned {
            if pinned.inventory.is_some() {
                result
                    .roots
                    .push(recovery::prepare(actor, source.clone(), pinned)?);
                continue;
            }
            if creature.requires_equipment {
                return Err("NPC legacy source lacks durable equipment proof".into());
            }
        }
        if !creature.requires_equipment {
            continue;
        }
        let mut identity = crate::region_activation::region_generator_identity(
            actor,
            prepared.fence.activation_epoch,
            prepared.fence.content_generation,
        );
        let mut hash = Sha256::new();
        hash.update(b"BetterACE static equipment v1");
        hash.update(world_epoch.to_le_bytes());
        hash.update(identity.random_identity);
        identity
            .random_identity
            .copy_from_slice(&hash.finalize()[..16]);
        let location = GeneratorLocation {
            cell: u32::from(prepared.fence.landblock) << 16 | 1,
            origin: [0.; 3],
            rotation: [0., 0., 0., 1.],
        };
        let intent = GeneratorSpawnIntent {
            key: GeneratorSpawnKey {
                generator: identity,
                profile_id: 0,
                occurrence: 1,
            },
            profile: GeneratorProfile {
                id: 0,
                probability: -1.,
                weenie_class_id: source.weenie_id,
                delay: None,
                init_create: 1,
                max_create: 1,
                when_create: 1,
                where_create: 1,
                stack_size: None,
                palette_id: None,
                shade: None,
                position: Default::default(),
            },
            destination: GeneratorDestination::Default(location),
            first_spawn: true,
            due_tick: 0,
            random_identity: identity.random_identity,
            random_key_version: random.key_version(),
        };
        let did = |id| {
            source
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value)
        };
        let wielded = did(32)
            .and_then(|id| prepared.catalog.wielded.get(&id))
            .map_or(&[][..], Vec::as_slice);
        let inventory = did(33).and_then(|id| prepared.catalog.treasure.get(&id));
        let items = crate::generator_equipment::materialize_creature_equipment_with_inventory(
            &source,
            &prepared.catalog.templates,
            wielded,
            inventory,
            drop_plain_wield,
            random,
            &intent,
        )
        .map_err(|error| {
            format!(
                "creature actor {:08x} WCID {}: {error}",
                actor.0, source.weenie_id
            )
        })?;
        result.count = result
            .count
            .checked_add(items.len())
            .ok_or("region equipment count overflow")?;
        if result.count > 4096 {
            return Err("region equipment count exceeds admission capacity".into());
        }
        result.roots.push(RootEquipment {
            restored: None,
            actor,
            source,
            items,
        });
    }
    Ok(result)
}
pub(super) fn bind(
    prepared: &PreparedRegionActivation,
    gear: &RegionEquipment,
    ids: &[EntityId],
    spells: &crate::generator_spell_assets::PreparedGeneratorSpellAssets,
) -> Result<BTreeMap<EntityId, bace_simulation::PreparedNpcLoadout>, String> {
    if ids.len() != gear.count {
        return Err("region reserved gear identities mismatch".into());
    }
    let mut offset = 0;
    let mut result = BTreeMap::new();
    let shapes = prepared
        .physical
        .iter()
        .filter_map(|(&id, p)| p.as_ref().ok().map(|p| (id, p.shape.clone())))
        .collect();
    for root in &gear.roots {
        let historical = prepared.npc_recovery.get(&root.actor);
        let selected = historical.map_or(prepared, |pin| pin.prepared.as_ref());
        let historical_shapes: Option<BTreeMap<_, _>> = historical.map(|_| {
            selected
                .physical
                .iter()
                .filter_map(|(&id, p)| p.as_ref().ok().map(|p| (id, p.shape.clone())))
                .collect()
        });
        let shapes = historical_shapes.as_ref().unwrap_or(&shapes);
        let spells = if historical.is_some() {
            selected.item_spells.as_ref().map_err(Clone::clone)?
        } else {
            spells
        };
        let prepared = selected;
        let items: Vec<_> = if let Some(saved) = &root.restored {
            saved
                .iter()
                .map(|i| EntityId(i.entity.object_id))
                .zip(root.items.iter().cloned())
                .collect()
        } else {
            let items = ids[offset..offset + root.items.len()]
                .iter()
                .copied()
                .zip(root.items.iter().cloned())
                .collect();
            offset += root.items.len();
            items
        };
        let creature = prepared
            .creatures
            .get(&root.source.weenie_id)
            .ok_or("missing region creature")?
            .as_ref()
            .map_err(Clone::clone)?;
        let mut loadout = if root.restored.is_some() {
            crate::generator_equipment::prepare_restored_creature_loadout(
                root.actor,
                &root.source,
                &items,
                creature,
                shapes,
            )?
        } else {
            crate::generator_equipment::prepare_creature_loadout_with_enchantments(
                root.actor,
                &root.source,
                &items,
                creature,
                shapes,
                Some(spells.borrowed()),
            )?
        };
        if let Some(saved) = &root.restored {
            recovery::restore_revisions(&mut loadout, saved)?;
        }
        result.insert(root.actor, loadout);
    }
    Ok(result)
}
pub(super) fn retain_sources(
    actor: EntityId,
    loadout: &bace_simulation::PreparedNpcLoadout,
    gear: &RegionEquipment,
    ids: &[EntityId],
    revision: u64,
    out: &mut BTreeMap<u32, crate::region_unload_saves::RegionItemSource>,
) -> Result<(), String> {
    let mut offset = 0;
    for root in &gear.roots {
        if root.actor == actor {
            if let Some(saved) = &root.restored {
                for item in saved {
                    if out
                        .insert(
                            item.entity.object_id,
                            crate::region_unload_saves::RegionItemSource {
                                item: item.clone(),
                                corpse: None,
                            },
                        )
                        .is_some()
                    {
                        return Err("duplicate restored NPC gear source".into());
                    }
                }
                return Ok(());
            }
            for (index, prepared) in root.items.iter().enumerate() {
                let id = ids[offset + index];
                let item = loadout
                    .items
                    .iter()
                    .find(|i| i.id == id)
                    .ok_or("prepared gear identity absent")?;
                let place = match item.place {
                    bace_inventory::ItemPlace::Contained {
                        container,
                        slot,
                        equipped,
                    } => bace_storage_codec::ItemPlacementV2::Contained {
                        container: container.0,
                        slot,
                        pack_slot: item.pack_slot,
                        equipped,
                    },
                    _ => return Err("region gear not contained".into()),
                };
                out.insert(
                    id.0,
                    crate::region_unload_saves::RegionItemSource {
                        item: crate::game_inventory::FrozenInventoryItem {
                            corpse: None,
                            construction: None,
                            source_destination: prepared.source_destination,
                            entity: bace_storage_codec::EntitySaveV1 {
                                object_id: id.0,
                                template_revision: revision,
                                mutation_revision: item.revision,
                                state: prepared.source.clone(),
                            },
                            placement: Some(place),
                            enchantments: vec![],
                            persisted_version: 0,
                        },
                        corpse: None,
                    },
                );
            }
            return Ok(());
        }
        if root.restored.is_none() {
            offset += root.items.len();
        }
    }
    Err("region gear source missing".into())
}

pub(super) fn spell_templates(
    gear: &RegionEquipment,
) -> BTreeMap<u32, Arc<bace_content::WeenieV1>> {
    gear.roots
        .iter()
        .filter(|root| root.restored.is_none())
        .flat_map(|r| r.items.iter())
        .enumerate()
        .map(|(index, item)| (index as u32, Arc::new(item.source.clone())))
        .collect()
}
