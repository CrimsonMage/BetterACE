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
                items,
                mut containers,
                roots,
                shapes: None,
                ..
            } => {
                let Materialized::Items(raw_items) = tree else {
                    return Err("contained item materialization mismatch".into());
                };
                let constructed = prepare_nested_leaf_creatures(
                    assets.as_deref_mut().map_err(|e| e.clone())?,
                    generation,
                    region,
                    &part,
                    raw_items,
                    &mut ready.sources,
                )?;
                for creature in &constructed {
                    // A Creature's inaccessible inventory is owned by its
                    // constructed loadout, not the generic container adapter.
                    let actor = creature.entity;
                    containers.retain(|container| container.id != actor);
                    containers.extend(
                        creature
                            .loadout
                            .containers
                            .iter()
                            .filter(|container| container.id == actor)
                            .copied(),
                    );
                }
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

fn prepare_nested_leaf_creatures(
    assets: &mut crate::region_activation::VerifiedRegionAssets,
    generation: &bace_storage_codec::PackGeneration,
    region: &PreparedRegionActivation,
    request: &GeneratorHostRequest,
    raw: &[bace_loot::PreparedContainerItem],
    sources: &mut [crate::region_unload_saves::RegionItemSource],
) -> Result<Vec<bace_simulation::PreparedConstructedCreature>, String> {
    if !matches!(
        request.intent.destination,
        GeneratorDestination::Contain { .. }
    ) || raw.len() != request.entities.len()
    {
        return Err("nested creature containment bounds".into());
    }
    let mut creatures = Vec::new();
    for (index, item) in raw.iter().enumerate() {
        let source = &item.source;
        if !crate::generator_preparation::is_creature_template(source.weenie_type) {
            continue;
        }
        if !matches!(source.weenie_type, 10 | 15)
            || item.parent_index.is_none()
            || !source.properties.create_list.is_empty()
            || !source.properties.generators.is_empty()
            || source
                .properties
                .data_ids
                .iter()
                .any(|p| matches!(p.id, 32 | 33) && p.value != 0)
        {
            return Err("nested creature loadout requires source construction".into());
        }
        let actor = request.entities[index];
        let template = region
            .creatures
            .get(&source.weenie_id)
            .ok_or("nested creature prepared template missing")?
            .as_ref()
            .map_err(Clone::clone)?;
        let mut loadout = crate::generator_equipment::prepare_creature_loadout(
            actor,
            source,
            &[],
            template,
            &std::collections::BTreeMap::new(),
        )?;
        assets.prepare_npc_loadout_motions(source, &mut loadout, &[], generation)?;
        let metadata = sources
            .iter_mut()
            .find(|row| row.item.entity.object_id == actor.0)
            .ok_or("nested creature retained source missing")?;
        metadata.item.construction = Some(super::construction::freeze(
            actor,
            source.weenie_type,
            &request.intent,
            &[],
            &loadout,
        )?);
        creatures.push(bace_simulation::PreparedConstructedCreature {
            entity: actor,
            weenie_type: source.weenie_type,
            loadout: Box::new(loadout),
        });
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
