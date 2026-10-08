//! Lossless cold projection of durable world forests and registry metadata.
use crate::{
    enchantment_saves::{EnchantmentDefinition, restore_saved_enchantments},
    game_inventory::FrozenInventoryItem,
    region_unload_saves::RegionItemSource,
};
use bace_persistence::{DurableItemPlace, LocatedSnapshot};
use bace_simulation::{
    PreparedRestoredConstructedCreature, PreparedWorldCorpse, PreparedWorldRegionItems,
    PreparedWorldRegionRoot,
};
use bace_storage_codec::{CorpseSaveV5, ItemPlacementV2, ItemSaveV2, ItemSaveV4, ItemSaveV5};
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub(crate) struct DecodedWorldItem {
    pub item: ItemSaveV4,
    pub corpse: Option<CorpseSaveV5>,
    pub source_destination: Option<u8>,
}
#[derive(Clone, Copy)]
pub(crate) struct WorldItemRestoreClock {
    pub epoch: u64,
    pub unix_seconds: i64,
    pub tick: u64,
}
pub(crate) fn decode(snapshot: &LocatedSnapshot) -> Result<DecodedWorldItem, String> {
    let decoded = decode_payload(snapshot)?;
    if decoded.item.entity.object_id != snapshot.aggregate.object_id
        || crate::game_inventory::durable(&decoded.item.placement) != snapshot.placement
    {
        return Err("world snapshot identity/placement mismatch".into());
    }
    Ok(decoded)
}
/// Checked binary decode; callers must separately validate relational identity
/// and placement, allowing recovery to retain the specific failure category.
pub(crate) fn decode_payload(snapshot: &LocatedSnapshot) -> Result<DecodedWorldItem, String> {
    let place = match snapshot.placement {
        DurableItemPlace::Contained {
            container,
            slot,
            pack_slot,
            equipped,
        } => Some(ItemPlacementV2::Contained {
            container,
            slot,
            pack_slot,
            equipped,
        }),
        DurableItemPlace::World { .. } => None,
        _ => return Err("removed item cannot enter a world forest".into()),
    };
    let info = bace_storage_codec::inspect(
        &snapshot.aggregate.bytes,
        bace_storage_codec::CodecLimits {
            max_payload_bytes: 2 * 1024 * 1024,
        },
    )
    .map_err(|e| e.to_string())?;
    let decoded = if info.kind == 102 {
        let corpse = CorpseSaveV5::decode_or_migrate(&snapshot.aggregate.bytes, place)
            .map_err(|e| e.to_string())?;
        let item = bace_storage_codec::ItemSaveV4 {
            previous: bace_storage_codec::ItemSaveV3 {
                previous: ItemSaveV2 {
                    entity: corpse.corpse.entity.clone(),
                    placement: corpse.placement.clone(),
                },
                enchantments: corpse.enchantments.clone(),
            },
            construction: None,
        };
        DecodedWorldItem {
            item,
            corpse: Some(corpse),
            source_destination: None,
        }
    } else {
        let saved = ItemSaveV5::decode_or_migrate(&snapshot.aggregate.bytes, place)
            .map_err(|e| e.to_string())?;
        DecodedWorldItem {
            item: saved.previous,
            corpse: None,
            source_destination: saved.source_destination,
        }
    };
    Ok(decoded)
}
fn decode_for_restore(snapshot: &LocatedSnapshot) -> Result<DecodedWorldItem, String> {
    let decoded = decode(snapshot)?;
    if crate::generator_preparation::is_creature_template(decoded.item.entity.state.weenie_type)
        && decoded.item.construction.is_none()
    {
        return Err("durable creature is missing its construction companion".into());
    }
    if decoded.item.construction.is_some()
        && !matches!(
            decoded.item.placement,
            ItemPlacementV2::Contained { equipped: 0, .. }
        )
    {
        return Err("constructed creature requires a contained root".into());
    }
    Ok(decoded)
}
pub(super) fn prepare(
    snapshots: &[LocatedSnapshot],
    prepared: &crate::region_activation::PreparedRegionActivation,
    assets: &mut crate::region_activation::VerifiedRegionAssets,
    spells: &Arc<bace_dat::SpellTable>,
    clock: WorldItemRestoreClock,
) -> Result<(PreparedWorldRegionItems, BTreeMap<u32, RegionItemSource>), String> {
    prepare_inner(snapshots, prepared, assets, spells, clock, None)
}
/// The committed native NoCorpse transaction may mark a quest-bearing world
/// root with GeneratorId = victim. This exception applies only to that exact
/// root in the already frozen transaction, never to cold generated remnants.
pub(crate) fn prepare_live_death(
    snapshots: &[LocatedSnapshot],
    prepared: &crate::region_activation::PreparedRegionActivation,
    assets: &mut crate::region_activation::VerifiedRegionAssets,
    spells: &Arc<bace_dat::SpellTable>,
    clock: WorldItemRestoreClock,
    no_corpse_victim: Option<EntityId>,
) -> Result<(PreparedWorldRegionItems, BTreeMap<u32, RegionItemSource>), String> {
    prepare_inner(snapshots, prepared, assets, spells, clock, no_corpse_victim)
}
fn prepare_inner(
    snapshots: &[LocatedSnapshot],
    prepared: &crate::region_activation::PreparedRegionActivation,
    assets: &mut crate::region_activation::VerifiedRegionAssets,
    spells: &Arc<bace_dat::SpellTable>,
    clock: WorldItemRestoreClock,
    no_corpse_victim: Option<EntityId>,
) -> Result<(PreparedWorldRegionItems, BTreeMap<u32, RegionItemSource>), String> {
    let WorldItemRestoreClock {
        epoch,
        unix_seconds,
        tick,
    } = clock;
    let mut out = PreparedWorldRegionItems::default();
    let mut sources = BTreeMap::new();
    let set_spells: BTreeSet<_> = spells
        .sets
        .values()
        .flat_map(|tiers| tiers.values().flatten().copied())
        .collect();
    let mut ancestry = BTreeMap::new();
    let mut saved_items = BTreeMap::new();
    let mut registry_entries = 0usize;
    for snapshot in snapshots {
        let decoded = decode_for_restore(snapshot)?;
        let saved = decoded.item;
        registry_entries = registry_entries
            .checked_add(saved.enchantments.len())
            .ok_or("world registry budget overflow")?;
        if registry_entries > 65536 {
            return Err("world registry entry capacity".into());
        }
        let id = EntityId(saved.entity.object_id);
        let place = match saved.placement {
            ItemPlacementV2::World(_) => bace_inventory::ItemPlace::World,
            ItemPlacementV2::Contained {
                container,
                slot,
                equipped,
                ..
            } => bace_inventory::ItemPlace::Contained {
                container: EntityId(container),
                slot,
                equipped,
            },
            ItemPlacementV2::Removed => return Err("removed world item".into()),
        };
        let (item, mut container) = crate::generator_items::prepare_inventory_item(
            &saved.entity.state,
            id,
            saved.entity.mutation_revision,
            place,
        )?;
        if let ItemPlacementV2::Contained { pack_slot, .. } = &saved.placement
            && *pack_slot != item.pack_slot
        {
            return Err("saved pack-slot property mismatch".into());
        }
        if let Some(c) = &mut container {
            c.generation = epoch;
        }
        out.containers.extend(container);
        ancestry.insert(id, place);
        if let ItemPlacementV2::World(p) = &saved.placement {
            let corpse = if let Some(c) = &decoded.corpse {
                let source = c
                    .source
                    .ok_or("legacy corpse has no exact source metadata")?;
                let operation = c
                    .operation
                    .ok_or("legacy corpse has no exact death operation")?;
                let mut restored = PreparedWorldCorpse::new(
                    operation,
                    EntityId(source),
                    saved.entity.state.weenie_id,
                    c.corpse.owner.map(EntityId),
                    crate::player_death_saves::corpse_expiry_tick(
                        c.corpse.expires_at,
                        unix_seconds,
                        tick,
                    )?,
                );
                restored.access = Some(bace_simulation::CorpseAccessProfile {
                    victim: c.access.victim.map(EntityId),
                    killer: c.access.killer.map(EntityId),
                    is_monster: c.access.is_monster,
                    generated_rare: c.access.generated_rare,
                    pk_death: c.access.pk_death,
                    looted: c.access.looted,
                    permittees: c.access.permittees.iter().copied().map(EntityId).collect(),
                });
                Some(restored)
            } else {
                None
            };
            out.roots.push(PreparedWorldRegionRoot {
                entity: id,
                location: bace_gameplay_api::GeneratorLocation {
                    cell: p.obj_cell_id,
                    origin: [p.position_x, p.position_y, p.position_z],
                    rotation: [p.rotation_x, p.rotation_y, p.rotation_z, p.rotation_w],
                },
                shape: assets.prepare_world_item_shape(&saved.entity.state)?,
                corpse,
            });
        } else if decoded.corpse.is_some() {
            return Err("contained corpse snapshot".into());
        }
        let registry =
            restore_saved_enchantments(&saved, 4096, |id| definition(spells, &set_spells, id))
                .map_err(|e| e.to_string())?;
        out.registries.push((id, registry));
        out.items.push(item);
        saved_items.insert(id, saved.clone());
        sources.insert(
            id.0,
            RegionItemSource {
                item: FrozenInventoryItem {
                    corpse: decoded.corpse.clone().map(Box::new),
                    construction: saved.construction.clone(),
                    source_destination: decoded.source_destination,
                    entity: saved.previous.previous.entity,
                    placement: Some(saved.previous.previous.placement),
                    persisted_version: snapshot.aggregate.persisted_version,
                    enchantments: saved.previous.enchantments,
                },
                corpse: decoded.corpse,
            },
        );
    }
    for (&actor, saved) in &saved_items {
        if saved.construction.is_some() {
            out.constructed.push(prepare_constructed_creature(
                actor,
                saved,
                &saved_items,
                &sources,
                &ancestry,
                prepared,
                assets,
            )?);
        }
    }
    let constructed_ids: BTreeSet<_> = out
        .constructed
        .iter()
        .flat_map(|entry| std::iter::once(entry.actor).chain(entry.children.iter().copied()))
        .collect();
    if saved_items.iter().any(|(id, saved)| {
        let quest_root = no_corpse_victim.is_some_and(|victim| {
            matches!(saved.placement, ItemPlacementV2::World(_))
                && saved
                    .entity
                    .state
                    .properties
                    .strings
                    .iter()
                    .any(|p| p.id == 33 && !p.value.is_empty())
                && saved
                    .entity
                    .state
                    .properties
                    .instance_ids
                    .iter()
                    .any(|p| p.id == 6 && p.value == victim.0)
        });
        !constructed_ids.contains(id)
            && !quest_root
            && saved
                .entity
                .state
                .properties
                .instance_ids
                .iter()
                .any(|p| p.id == 6)
    }) {
        return Err("unreconciled generated world item".into());
    }
    for root in &mut out.roots {
        if let Some(corpse) = &mut root.corpse {
            for &id in ancestry.keys().filter(|&&id| id != root.entity) {
                let mut current = id;
                for _ in 0..64 {
                    match ancestry.get(&current) {
                        Some(bace_inventory::ItemPlace::Contained { container, .. }) => {
                            current = *container;
                            if current == root.entity {
                                corpse.state.items.push(id);
                                break;
                            }
                        }
                        _ => break,
                    }
                }
            }
        }
    }
    Ok((out, sources))
}
pub(crate) fn definition(
    table: &Arc<bace_dat::SpellTable>,
    sets: &BTreeSet<u32>,
    id: u32,
) -> Option<EnchantmentDefinition> {
    let s = table.spells.get(&id)?;
    let school = match s.school {
        1 => bace_magic::MagicSchool::War,
        2 => bace_magic::MagicSchool::Life,
        3 => bace_magic::MagicSchool::Item,
        4 => bace_magic::MagicSchool::Creature,
        5 => bace_magic::MagicSchool::Void,
        _ => return None,
    };
    let (_, degrade_modifier, degrade_limit) = s.enchantment?;
    Some(EnchantmentDefinition {
        school,
        is_set_spell: sets.contains(&id),
        // Pinned PropertiesEnchantmentRegistryExtensions.Level8AuraSelfSpells.
        is_level8_aura: [4395, 4400, 4405, 4414, 4417, 4418].contains(&id),
        category: u16::try_from(s.category).ok()?,
        power: s.power,
        degrade_modifier,
        degrade_limit,
        stat_type: 0,
        stat_key: 0,
        beneficial: s.flags & 4 != 0,
    })
}

pub(super) fn source_size(source: &RegionItemSource) -> Result<usize, String> {
    if let Some(corpse) = &source.corpse {
        return corpse.encode().map(|b| b.len()).map_err(|e| e.to_string());
    }
    ItemSaveV5 {
        previous: ItemSaveV4 {
            previous: bace_storage_codec::ItemSaveV3 {
                previous: ItemSaveV2 {
                    entity: source.item.entity.clone(),
                    placement: source
                        .item
                        .placement
                        .clone()
                        .ok_or("region source requires placement")?,
                },
                enchantments: source.item.enchantments.clone(),
            },
            construction: source.item.construction.clone(),
        },
        source_destination: source.item.source_destination,
    }
    .encode()
    .map(|b| b.len())
    .map_err(|e| e.to_string())
}

fn constructed_source_parent_valid(
    root: &ItemSaveV4,
    saved: &BTreeMap<EntityId, ItemSaveV4>,
    generator: u32,
) -> bool {
    let Some(source_parent) = root
        .entity
        .state
        .properties
        .instance_ids
        .iter()
        .find(|p| p.id == 6)
        .map(|p| p.value)
    else {
        return false;
    };
    if source_parent == generator {
        return true;
    }
    let ItemPlacementV2::Contained { container, .. } = root.placement else {
        return false;
    };
    let mut current = container;
    for _ in 0..64 {
        let Some(parent) = saved.get(&EntityId(current)) else {
            return false;
        };
        if current == source_parent {
            return true;
        }
        let ItemPlacementV2::Contained { container, .. } = parent.placement else {
            return false;
        };
        current = container;
    }
    false
}

fn prepare_constructed_creature(
    actor: EntityId,
    root: &ItemSaveV4,
    saved: &BTreeMap<EntityId, ItemSaveV4>,
    sources: &BTreeMap<u32, RegionItemSource>,
    places: &BTreeMap<EntityId, bace_inventory::ItemPlace>,
    prepared: &crate::region_activation::PreparedRegionActivation,
    assets: &mut crate::region_activation::VerifiedRegionAssets,
) -> Result<PreparedRestoredConstructedCreature, String> {
    use bace_gameplay_api::{
        GeneratorDestination, GeneratorIdentity, GeneratorPositionSpec, GeneratorProfile,
        GeneratorSpawnIntent, GeneratorSpawnKey,
    };
    use bace_loot::PreparedCreatureEquipment;
    let construction = root
        .construction
        .as_ref()
        .ok_or("creature construction missing")?;
    let ItemPlacementV2::Contained {
        container,
        equipped: 0,
        ..
    } = root.placement
    else {
        return Err("constructed root is not contained".into());
    };
    if !constructed_source_parent_valid(root, saved, construction.origin.generator) {
        return Err("constructed root generator provenance mismatch".into());
    }
    let mut descendants = Vec::new();
    let mut owned = Vec::new();
    for (&id, value) in saved {
        if id == actor {
            continue;
        }
        let mut current = id;
        let mut depth = 0usize;
        let mut inner_creature = false;
        let mut found = false;
        for _ in 0..64 {
            let Some(place) = places.get(&current) else {
                break;
            };
            let bace_inventory::ItemPlace::Contained { container, .. } = place else {
                break;
            };
            if *container == actor {
                found = true;
                break;
            }
            inner_creature |= saved
                .get(container)
                .is_some_and(|row| row.construction.is_some());
            current = *container;
            depth += 1;
        }
        if found {
            descendants.push(id);
            if !inner_creature {
                owned.push((depth, id, value));
            }
        }
    }
    if descendants.len() > 1023 || owned.len() > 1023 {
        return Err("constructed creature forest capacity".into());
    }
    let order: BTreeMap<_, _> = construction
        .equipment_order
        .iter()
        .enumerate()
        .map(|(index, id)| (EntityId(*id), index as u32))
        .collect();
    owned.sort_by_key(|(depth, id, _)| (*depth, order.get(id).copied().unwrap_or(u32::MAX), *id));
    let actual_equipment: BTreeSet<_> = owned
        .iter()
        .filter_map(|(_, id, row)| {
            matches!(row.placement, ItemPlacementV2::Contained { equipped, .. } if equipped != 0)
                .then_some(*id)
        })
        .collect();
    if actual_equipment != order.keys().copied().collect() {
        return Err("constructed equipment order differs from saved placement".into());
    }
    let indices: BTreeMap<_, _> = owned
        .iter()
        .enumerate()
        .map(|(n, (_, id, _))| (*id, n))
        .collect();
    let death_ids: BTreeSet<_> = construction
        .death_roster
        .iter()
        .map(|r| EntityId(r.entity))
        .collect();
    if death_ids.iter().any(|id| !indices.contains_key(id)) {
        return Err("constructed death roster contains an unrelated item".into());
    }
    let mut gear = Vec::with_capacity(owned.len());
    let mut exact_revisions = BTreeMap::new();
    for (_, id, row) in &owned {
        let ItemPlacementV2::Contained {
            container: parent,
            slot,
            equipped,
            ..
        } = row.placement
        else {
            return Err("constructed child is not contained".into());
        };
        let parent_index = (parent != actor.0)
            .then(|| indices.get(&EntityId(parent)).copied())
            .flatten();
        if parent != actor.0 && parent_index.is_none() {
            return Err("constructed child parent crosses owner boundary".into());
        }
        let mut source = row.entity.state.clone();
        // Saved registries are the only enchantment owner on restore. Preparing
        // constructor spell casts a second time would duplicate active auras.
        source.properties.spell_book.clear();
        gear.push((
            *id,
            PreparedCreatureEquipment {
                source,
                source_destination: sources
                    .get(&id.0)
                    .ok_or("constructed child durable source missing")?
                    .item
                    .source_destination,
                wielded_location: equipped,
                death_drop: death_ids.contains(id),
                parent_index,
                inventory_slot: slot,
                equip_order: order.get(id).copied(),
            },
        ));
        exact_revisions.insert(*id, row.entity.mutation_revision);
    }
    let mut template = prepared
        .creatures
        .get(&root.entity.state.weenie_id)
        .ok_or("saved creature template absent from accepted region")?
        .as_ref()
        .map_err(Clone::clone)?
        .clone();
    let mut shapes: BTreeMap<_, _> = prepared
        .physical
        .iter()
        .filter_map(|(&id, shape)| shape.as_ref().ok().map(|shape| (id, shape.shape.clone())))
        .collect();
    for (_, row) in &gear {
        if matches!(row.wielded_location, 0x400000 | 0x800000)
            && !shapes.contains_key(&row.source.weenie_id)
        {
            shapes.insert(
                row.source.weenie_id,
                assets.prepare_world_item_shape(&row.source)?,
            );
        }
    }
    let mut loadout = crate::generator_equipment::prepare_creature_loadout(
        actor,
        &root.entity.state,
        &gear,
        &template,
        &shapes,
    )?;
    let profile = Arc::make_mut(&mut loadout.profile);
    for stamp in &mut profile.equipment {
        stamp.revision = *exact_revisions
            .get(&EntityId(stamp.entity))
            .ok_or("restored equipment stamp absent")?;
    }
    for weapon in [
        &mut profile.main,
        &mut profile.offhand,
        &mut profile.launcher,
        &mut profile.ammunition,
        &mut profile.gloves,
        &mut profile.boots,
    ]
    .into_iter()
    .flatten()
    {
        weapon.revision = *exact_revisions
            .get(&EntityId(weapon.entity))
            .ok_or("restored combat item absent")?;
    }
    loadout.death_ids = construction
        .death_roster
        .iter()
        .map(|r| EntityId(r.entity))
        .collect();
    loadout.death_items = loadout
        .death_ids
        .iter()
        .map(|id| {
            saved
                .get(id)
                .map(|item| item.entity.state.clone())
                .ok_or("restored death item absent".to_string())
        })
        .collect::<Result<_, _>>()?;
    let death_indices: BTreeMap<_, _> = loadout
        .death_ids
        .iter()
        .enumerate()
        .map(|(n, id)| (*id, n))
        .collect();
    loadout.death_parents = construction
        .death_roster
        .iter()
        .map(|r| {
            r.parent
                .map(|id| {
                    death_indices
                        .get(&EntityId(id))
                        .copied()
                        .ok_or("restored death parent absent".to_string())
                })
                .transpose()
        })
        .collect::<Result<_, _>>()?;
    let equipment = profile
        .equipment
        .iter()
        .map(|stamp| {
            let row = saved
                .get(&EntityId(stamp.entity))
                .ok_or("restored equipment source absent")?;
            Ok(bace_combat::preparation::PhysicalEquipmentSource {
                entity: stamp.entity,
                revision: stamp.revision,
                location: stamp.location,
                weenie: Arc::new(row.entity.state.clone()),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    assets.prepare_npc_loadout_motions(
        &root.entity.state,
        &mut loadout,
        &equipment,
        &prepared.generation,
    )?;
    template.physical = Some(loadout.profile.clone());
    template.combat_assets = loadout.combat_assets;
    template.death_motions = loadout.death_motions;
    template.physical_motions = loadout.physical_motions;
    template.locomotion_styles = loadout.locomotion_styles;
    if let Some(policy) = &mut template.ace_loot {
        policy.initial_items = loadout.death_items;
        policy.initial_ids = loadout.death_ids;
        policy.initial_parents = loadout.death_parents;
    } else if !construction.death_roster.is_empty() {
        return Err("saved creature death roster has no ACE loot owner".into());
    }
    let origin = &construction.origin;
    let key = GeneratorSpawnKey {
        generator: GeneratorIdentity {
            entity: EntityId(origin.generator),
            incarnation: origin.incarnation,
            content_revision: origin.content_revision,
            random_identity: origin.random_identity,
        },
        profile_id: origin.profile,
        occurrence: origin.occurrence,
    };
    let intent = GeneratorSpawnIntent {
        key,
        profile: GeneratorProfile {
            id: origin.profile,
            probability: -1.0,
            weenie_class_id: root.entity.state.weenie_id,
            delay: None,
            init_create: 1,
            max_create: 1,
            when_create: 1,
            where_create: 8,
            stack_size: None,
            palette_id: None,
            shade: None,
            position: GeneratorPositionSpec::default(),
        },
        destination: GeneratorDestination::Contain {
            container: EntityId(container),
        },
        first_spawn: false,
        due_tick: 0,
        random_identity: origin.random_identity,
        random_key_version: origin.random_key_version,
    };
    Ok(PreparedRestoredConstructedCreature {
        actor,
        landblock: prepared.fence.landblock,
        intent,
        template,
        children: descendants,
    })
}

#[cfg(test)]
mod tests;
