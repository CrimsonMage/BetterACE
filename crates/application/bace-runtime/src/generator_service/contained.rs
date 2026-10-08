//! Source-ordered mixed Contain results form one atomic inventory forest.
use super::*;
use bace_gameplay_api::GeneratorDestination;
use materialization::{Materialized, Ready};
pub(super) fn bind(
    mut assets: Result<&mut crate::region_activation::VerifiedRegionAssets, String>,
    table: &mut Option<Arc<bace_dat::SpellTable>>,
    generation: &bace_storage_codec::PackGeneration,
    region: &PreparedRegionActivation,
    request: &GeneratorHostRequest,
    raw: &[Materialized],
) -> Result<Ready, String> {
    let mut forest = bace_simulation::PreparedContainedForest {
        roots: vec![],
        items: vec![],
        containers: vec![],
        creatures: vec![],
    };
    let mut sources = Vec::new();
    let mut offset = 0;
    let mut slots = request.next_slots.ok_or("missing Contain slot snapshot")?;
    for tree in raw {
        if matches!(tree, Materialized::Mixed(_)) {
            return Err("nested mixed materialization".into());
        }
        let mut part = request.clone();
        part.entities = request
            .entities
            .get(offset..offset + tree.count())
            .ok_or("contained identity count")?
            .to_vec();
        offset += tree.count();
        part.next_slots = Some(slots);
        let mut ready = materialization::bind(
            assets.as_deref_mut().map_err(|e| e.clone()),
            table,
            generation,
            region,
            &part,
            tree,
        )?;
        let roots = match ready.action {
            GeneratorAction::AdmitContainedCreature { prepared, .. } => {
                let id = prepared.root.id;
                forest.roots.push(id);
                forest.items.push(prepared.root);
                forest.items.extend(prepared.loadout.items.iter().cloned());
                forest
                    .containers
                    .extend(prepared.loadout.containers.iter().copied());
                forest
                    .creatures
                    .push(bace_simulation::PreparedConstructedCreature {
                        entity: id,
                        weenie_type: prepared.weenie_type,
                        loadout: prepared.loadout,
                    });
                vec![id]
            }
            GeneratorAction::AdmitItemTrees {
                mut items,
                mut containers,
                roots,
                shapes: None,
                ..
            } => {
                let (raw_items, nested) = match tree {
                    Materialized::Items(items) => (items.as_slice(), &[][..]),
                    Materialized::NestedItems { items, creatures } => {
                        (items.as_slice(), creatures.as_slice())
                    }
                    _ => return Err("contained item materialization mismatch".into()),
                };
                let constructed = prepare_nested_creatures(
                    assets.as_deref_mut().map_err(|e| e.clone())?,
                    table,
                    generation,
                    region,
                    &part,
                    raw_items,
                    nested,
                    &mut items,
                    &mut containers,
                    &mut ready.sources,
                )?;
                forest.creatures.extend(constructed);
                forest.roots.extend(&roots);
                forest.items.extend(items);
                forest.containers.extend(containers);
                roots
            }
            _ => return Err("invalid contained forest root".into()),
        };
        for root in roots {
            let item = forest
                .items
                .iter()
                .find(|i| i.id == root)
                .ok_or("contained root missing")?;
            let slot = if item.pack_slot {
                &mut slots.1
            } else {
                &mut slots.0
            };
            *slot = slot.checked_add(1).ok_or("contained slot overflow")?;
        }
        sources.extend(ready.sources);
    }
    if offset != request.entities.len() || !forest.valid_bounds() {
        return Err("contained forest bounds".into());
    }
    Ok(Ready {
        action: GeneratorAction::AdmitContainedForest {
            key: request.intent.key,
            prepared: Box::new(forest),
        },
        sources,
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_nested_creatures(
    assets: &mut crate::region_activation::VerifiedRegionAssets,
    table: &mut Option<Arc<bace_dat::SpellTable>>,
    generation: &bace_storage_codec::PackGeneration,
    region: &PreparedRegionActivation,
    request: &GeneratorHostRequest,
    raw: &[bace_loot::PreparedContainerItem],
    nested: &[materialization::NestedCreatureMaterialization],
    items: &mut Vec<bace_inventory::InventoryItem>,
    containers: &mut Vec<bace_inventory::InventoryContainer>,
    sources: &mut Vec<crate::region_unload_saves::RegionItemSource>,
) -> Result<Vec<bace_simulation::PreparedConstructedCreature>, String> {
    if !matches!(
        request.intent.destination,
        GeneratorDestination::Contain { .. }
    ) || raw.len() > request.entities.len()
    {
        return Err("nested creature containment bounds".into());
    }
    let expected_count = raw.len()
        + nested
            .iter()
            .map(|creature| creature.gear.len())
            .sum::<usize>();
    if expected_count != request.entities.len() {
        return Err("nested Creature identity count".into());
    }
    let mut creatures = Vec::new();
    let mut next_gear = raw.len();
    let mut seen_roots = std::collections::BTreeSet::new();
    for sidecar in nested {
        let source = &sidecar.source;
        if sidecar.gear_start != next_gear
            || sidecar.root_index >= sidecar.gear_start
            || !seen_roots.insert(sidecar.root_index)
        {
            return Err("nested Creature identity partition".into());
        }
        if !crate::generator_preparation::is_creature_template(source.weenie_type) {
            return Err("nested Creature sidecar type".into());
        }
        if !matches!(source.weenie_type, 10 | 15) || !source.properties.generators.is_empty() {
            return Err("nested Creature subtype requires its lifecycle owner".into());
        }
        let actor = *request
            .entities
            .get(sidecar.root_index)
            .ok_or("nested Creature root identity")?;
        if items
            .iter()
            .find(|item| item.id == actor)
            .is_none_or(|item| {
                item.template != source.weenie_id
                    || !matches!(
                        item.place,
                        bace_inventory::ItemPlace::Contained { equipped: 0, .. }
                    )
            })
        {
            return Err("nested Creature root is not in prior forest".into());
        }
        let end = sidecar
            .gear_start
            .checked_add(sidecar.gear.len())
            .ok_or("nested Creature gear identity overflow")?;
        next_gear = end;
        let gear_ids = request
            .entities
            .get(sidecar.gear_start..end)
            .ok_or("nested Creature gear identity slice")?;
        let gear: Vec<_> = gear_ids
            .iter()
            .copied()
            .zip(sidecar.gear.iter().cloned())
            .collect();
        let template = region
            .creatures
            .get(&source.weenie_id)
            .ok_or("nested creature prepared template missing")?
            .as_ref()
            .map_err(Clone::clone)?;
        if table.is_none() {
            *table = Some(assets.prepare_world_spell_table()?);
        }
        let templates = gear
            .iter()
            .enumerate()
            .map(|(index, (_, item))| (index as u32, Arc::new(item.source.clone())))
            .collect();
        let spells = crate::generator_spell_assets::prepare_generator_spell_rows(
            generation,
            table.as_ref().ok_or("missing client spell table")?.clone(),
            &templates,
        )?;
        let mut shapes: std::collections::BTreeMap<_, _> = region
            .physical
            .iter()
            .filter_map(|(&id, prepared)| {
                prepared
                    .as_ref()
                    .ok()
                    .map(|value| (id, value.shape.clone()))
            })
            .collect();
        for (_, item) in &gear {
            if matches!(item.wielded_location, 0x400000 | 0x800000) {
                shapes.insert(
                    item.source.weenie_id,
                    assets.prepare_world_item_shape(&item.source)?,
                );
            }
        }
        let mut loadout = crate::generator_equipment::prepare_creature_loadout_with_enchantments(
            actor,
            source,
            &gear,
            template,
            &shapes,
            Some(spells.borrowed()),
        )?;
        let equipment = loadout
            .profile
            .equipment
            .iter()
            .map(|stamp| {
                let raw = gear
                    .iter()
                    .find(|(id, _)| id.0 == stamp.entity)
                    .ok_or("nested Creature equipped source absent")?;
                Ok(bace_combat::preparation::PhysicalEquipmentSource {
                    entity: stamp.entity,
                    revision: stamp.revision,
                    location: stamp.location,
                    weenie: Arc::new(raw.1.source.clone()),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        assets.prepare_npc_loadout_motions(source, &mut loadout, &equipment, generation)?;
        for ((id, raw), item) in gear.iter().zip(&loadout.items) {
            if *id != item.id {
                return Err("nested Creature gear projection identity".into());
            }
            let parent = raw
                .parent_index
                .and_then(|index| gear_ids.get(index))
                .copied()
                .unwrap_or(actor);
            let mut source = raw.source.clone();
            materialization::set(&mut source.properties.instance_ids, 6, parent.0);
            sources.push(materialization::source_for(
                item,
                source,
                request.intent.key.generator.content_revision,
                raw.source_destination,
            ));
        }
        items.extend(loadout.items.iter().cloned());
        for container in &loadout.containers {
            containers.retain(|existing| existing.id != container.id);
            containers.push(*container);
        }
        let metadata = sources
            .iter_mut()
            .find(|row| row.item.entity.object_id == actor.0)
            .ok_or("nested creature retained source missing")?;
        metadata.item.construction = Some(super::construction::freeze(
            actor,
            source.weenie_type,
            &request.intent,
            &gear,
            &loadout,
        )?);
        creatures.push(bace_simulation::PreparedConstructedCreature {
            entity: actor,
            weenie_type: source.weenie_type,
            loadout: Box::new(loadout),
        });
    }
    if next_gear != request.entities.len()
        || sources
            .iter()
            .filter(|row| {
                crate::generator_preparation::is_creature_template(
                    row.item.entity.state.weenie_type,
                )
            })
            .count()
            != creatures.len()
    {
        return Err("nested Creature source missing construction sidecar".into());
    }
    // Each constructed owner retains every physical descendant, even when a
    // deeper constructed creature exclusively owns its own equipment and aura.
    let graph: std::collections::BTreeMap<_, _> =
        items.iter().map(|item| (item.id, item)).collect();
    for creature in &mut creatures {
        creature.loadout.items = items
            .iter()
            .filter(|item| {
                let mut current = item.id;
                for _ in 0..64 {
                    if current == creature.entity {
                        return item.id != creature.entity;
                    }
                    let Some(parent) = graph.get(&current) else {
                        return false;
                    };
                    let bace_inventory::ItemPlace::Contained { container, .. } = parent.place
                    else {
                        return false;
                    };
                    current = container;
                }
                false
            })
            .cloned()
            .collect();
    }
    Ok(creatures)
}
pub(super) fn valid_receipt(
    action: &GeneratorAction,
    receipt: &bace_simulation::GeneratorItemAdmission,
) -> bool {
    let GeneratorAction::AdmitContainedForest { key, prepared } = action else {
        return false;
    };
    let expected: std::collections::BTreeSet<_> = prepared.items.iter().map(|i| i.id).collect();
    receipt.key == *key
        && receipt.roots == prepared.roots
        && receipt.failed_roots.is_empty()
        && receipt.entities.len() == expected.len()
        && receipt
            .entities
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            == expected
}
