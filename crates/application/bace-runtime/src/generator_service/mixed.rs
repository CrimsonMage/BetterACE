//! Cold source-order forest preparation, including creature-valued wielded loot.
use super::*;
use bace_content::WeenieV1;
use bace_gameplay_api::GeneratorDestination;
use bace_simulation::PreparedMixedGeneratorRoot as Root;
use materialization::{Materialized, NestedCreatureMaterialization, Ready};
use std::collections::{BTreeSet, VecDeque};
pub(super) fn materialize(
    region: &PreparedRegionActivation,
    request: &GeneratorHostRequest,
    roots: Vec<WeenieV1>,
    random: &bace_random::RandomRoot,
    drop_plain_wield: bool,
) -> Result<Materialized, String> {
    let mut forest = Vec::new();
    let mut count = 0usize;
    for (ordinal, source) in roots.into_iter().enumerate() {
        let tree = if crate::generator_preparation::is_creature_template(source.weenie_type) {
            if !matches!(
                request.intent.destination,
                GeneratorDestination::Default(_)
                    | GeneratorDestination::Specific(_)
                    | GeneratorDestination::Scatter { .. }
                    | GeneratorDestination::Contain { .. }
            ) {
                return Err(
                    "creature-valued container/vendor object admission requires that owner".into(),
                );
            }
            if matches!(
                request.intent.destination,
                GeneratorDestination::Contain { .. }
            ) && !matches!(source.weenie_type, 10 | 15)
            {
                return Err("contained specialized Creature requires its subtype lifecycle".into());
            }
            let did = |id| {
                source
                    .properties
                    .data_ids
                    .iter()
                    .find(|p| p.id == id && p.value != 0)
                    .map(|p| p.value)
            };
            let wielded = did(32)
                .and_then(|id| region.catalog.wielded.get(&id))
                .map_or(&[][..], Vec::as_slice);
            let inventory = did(33).and_then(|id| region.catalog.treasure.get(&id));
            let mut intent = request.intent.clone();
            if ordinal > 0 {
                let mut stream = bace_spawning::generator_event_stream(
                    random,
                    &intent,
                    0x30000 + ordinal as u32,
                )
                .map_err(|e| format!("equipment stream: {e:?}"))?;
                intent.random_identity[..8].copy_from_slice(
                    &stream
                        .next_u64()
                        .map_err(|e| format!("equipment stream: {e:?}"))?
                        .to_le_bytes(),
                );
                intent.random_identity[8..].copy_from_slice(
                    &stream
                        .next_u64()
                        .map_err(|e| format!("equipment stream: {e:?}"))?
                        .to_le_bytes(),
                );
            }
            let gear = crate::generator_equipment::materialize_creature_equipment_with_inventory(
                &source,
                &region.catalog.templates,
                wielded,
                inventory,
                drop_plain_wield,
                random,
                &intent,
            )?;
            if matches!(
                request.intent.destination,
                GeneratorDestination::Contain { .. }
            ) {
                let items = vec![bace_loot::PreparedContainerItem {
                    source,
                    source_destination: None,
                    parent_index: None,
                    generator_parent_index: None,
                    inventory_slot: 0,
                }];
                let creatures = materialize_nested_creatures(
                    region,
                    request,
                    &items,
                    Some(gear),
                    ordinal,
                    random,
                    drop_plain_wield,
                )?;
                Materialized::NestedItems { items, creatures }
            } else {
                Materialized::Creature {
                    source: Arc::new(source),
                    gear,
                }
            }
        } else {
            let items = bace_loot::materialize_container_tree(source, &region.catalog.templates)
                .map_err(|e| format!("generator container contents: {e:?}"))?;
            let creatures = materialize_nested_creatures(
                region,
                request,
                &items,
                None,
                ordinal,
                random,
                drop_plain_wield,
            )?;
            if creatures.is_empty() {
                Materialized::Items(items)
            } else {
                Materialized::NestedItems { items, creatures }
            }
        };
        count = count
            .checked_add(tree.count())
            .filter(|n| *n <= 1024)
            .ok_or("generator forest identity capacity")?;
        forest.push(tree);
    }
    let nested_creature = forest
        .iter()
        .any(|tree| matches!(tree, Materialized::NestedItems { .. }));
    if !nested_creature && forest.iter().all(|r| matches!(r, Materialized::Items(_))) {
        let mut flat = Vec::new();
        for tree in forest {
            let Materialized::Items(items) = tree else {
                return Err("invalid item forest".into());
            };
            let offset = flat.len();
            for mut item in items {
                item.parent_index = item.parent_index.map(|n| n + offset);
                item.generator_parent_index = item.generator_parent_index.map(|n| n + offset);
                flat.push(item);
            }
        }
        return Ok(Materialized::Items(flat));
    }
    if !nested_creature
        && forest.len() == 1
        && (request.intent.profile.where_create & 64 == 0
            || matches!(
                request.intent.destination,
                GeneratorDestination::Contain { .. }
            ))
    {
        return forest.pop().ok_or("empty forest".into());
    }
    Ok(Materialized::Mixed(forest))
}
fn materialize_nested_creatures(
    region: &PreparedRegionActivation,
    request: &GeneratorHostRequest,
    items: &[bace_loot::PreparedContainerItem],
    root_gear: Option<Vec<bace_loot::PreparedCreatureEquipment>>,
    ordinal: usize,
    random: &bace_random::RandomRoot,
    drop_plain_wield: bool,
) -> Result<Vec<NestedCreatureMaterialization>, String> {
    let mut pending = VecDeque::new();
    let mut next_index = items.len();
    let mut creatures = Vec::new();
    if let Some(gear) = root_gear {
        let source = items
            .first()
            .ok_or("contained Creature root source missing")?
            .source
            .clone();
        if items.len() != 1
            || !matches!(source.weenie_type, 10 | 15)
            || !source.properties.generators.is_empty()
        {
            return Err("contained Creature root requires its subtype owner".into());
        }
        next_index = next_index
            .checked_add(gear.len())
            .filter(|count| *count <= 1024)
            .ok_or("contained Creature gear identity capacity")?;
        for (index, item) in gear.iter().enumerate() {
            if !item.source.properties.generators.is_empty() {
                return Err("constructed Creature child generator requires its owner".into());
            }
            if crate::generator_preparation::is_creature_template(item.source.weenie_type) {
                if item.wielded_location != 0 {
                    return Err("equipped nested Creature requires its subtype owner".into());
                }
                pending.push_back((items.len() + index, item.source.clone(), 1usize));
            }
        }
        creatures.push(NestedCreatureMaterialization {
            root_index: 0,
            gear_start: items.len(),
            source,
            gear,
        });
    } else {
        for (index, item) in items.iter().enumerate() {
            if crate::generator_preparation::is_creature_template(item.source.weenie_type) {
                if item.parent_index.is_none() {
                    return Err("nested Creature root has no container parent".into());
                }
                pending.push_back((index, item.source.clone(), 0usize));
            }
        }
    }
    if pending.is_empty() && creatures.is_empty() {
        return Ok(Vec::new());
    }
    if !matches!(
        request.intent.destination,
        GeneratorDestination::Contain { .. }
    ) {
        return Err("nested Creature requires Contain owner".into());
    }
    while let Some((root_index, source, depth)) = pending.pop_front() {
        if depth >= 16
            || !matches!(source.weenie_type, 10 | 15)
            || !source.properties.generators.is_empty()
        {
            return Err("nested Creature subtype or generator requires its lifecycle owner".into());
        }
        let did = |id| {
            source
                .properties
                .data_ids
                .iter()
                .find(|property| property.id == id && property.value != 0)
                .map(|property| property.value)
        };
        let wielded = did(32)
            .and_then(|id| region.catalog.wielded.get(&id))
            .map_or(&[][..], Vec::as_slice);
        let inventory = did(33).and_then(|id| region.catalog.treasure.get(&id));
        let purpose = u32::try_from(ordinal)
            .ok()
            .and_then(|ordinal| ordinal.checked_mul(1024))
            .and_then(|offset| offset.checked_add(u32::try_from(root_index).ok()?))
            .and_then(|offset| offset.checked_add(0x40000))
            .ok_or("nested Creature random stream bound")?;
        let mut intent = request.intent.clone();
        let mut stream = bace_spawning::generator_event_stream(random, &intent, purpose)
            .map_err(|e| format!("nested Creature stream: {e:?}"))?;
        intent.random_identity[..8].copy_from_slice(
            &stream
                .next_u64()
                .map_err(|e| format!("nested Creature stream: {e:?}"))?
                .to_le_bytes(),
        );
        intent.random_identity[8..].copy_from_slice(
            &stream
                .next_u64()
                .map_err(|e| format!("nested Creature stream: {e:?}"))?
                .to_le_bytes(),
        );
        let gear = crate::generator_equipment::materialize_creature_equipment_with_inventory(
            &source,
            &region.catalog.templates,
            wielded,
            inventory,
            drop_plain_wield,
            random,
            &intent,
        )?;
        let end = next_index
            .checked_add(gear.len())
            .filter(|end| *end <= 1024)
            .ok_or("nested Creature identity capacity")?;
        for (index, item) in gear.iter().enumerate() {
            if !item.source.properties.generators.is_empty() {
                return Err("constructed Creature child generator requires its owner".into());
            }
            if crate::generator_preparation::is_creature_template(item.source.weenie_type) {
                if item.wielded_location != 0 {
                    return Err("equipped nested Creature requires its subtype owner".into());
                }
                pending.push_back((next_index + index, item.source.clone(), depth + 1));
            }
        }
        creatures.push(NestedCreatureMaterialization {
            root_index,
            gear_start: next_index,
            source,
            gear,
        });
        next_index = end;
    }
    Ok(creatures)
}
pub(super) fn bind(
    mut assets: Result<&mut crate::region_activation::VerifiedRegionAssets, String>,
    table: &mut Option<Arc<bace_dat::SpellTable>>,
    generation: &bace_storage_codec::PackGeneration,
    region: &PreparedRegionActivation,
    request: &GeneratorHostRequest,
    raw: &[Materialized],
) -> Result<Ready, String> {
    if matches!(
        request.intent.destination,
        GeneratorDestination::Contain { .. }
    ) {
        return super::contained::bind(assets, table, generation, region, request, raw);
    }
    let mut roots = Vec::new();
    let mut sources = Vec::new();
    let mut offset = 0;
    for tree in raw {
        if matches!(tree, Materialized::Mixed(_)) {
            return Err("nested mixed materialization".into());
        }
        let mut part = request.clone();
        part.entities = request.entities[offset..offset + tree.count()].to_vec();
        offset += tree.count();
        let root = part.entities[0];
        let ready = materialization::bind(
            assets.as_deref_mut().map_err(|e| e.clone()),
            table,
            generation,
            region,
            &part,
            tree,
        )?;
        let value = match ready.action {
            GeneratorAction::AdmitItemTrees {
                items,
                containers,
                roots,
                shapes: Some(shapes),
                ..
            } if roots == [root] && shapes.len() == 1 => Root::Item {
                root,
                items,
                containers,
                shape: shapes[0].clone(),
            },
            GeneratorAction::AdmitCreature { loadout, .. } => {
                let Materialized::Creature { source, .. } = tree else {
                    return Err("creature materialization mismatch".into());
                };
                Root::Creature {
                    root,
                    template: source.weenie_id,
                    loadout,
                }
            }
            _ => return Err("unsupported mixed generator destination".into()),
        };
        roots.push(value);
        sources.extend(ready.sources);
    }
    Ok(Ready {
        action: GeneratorAction::AdmitMixedTrees {
            key: request.intent.key,
            roots,
        },
        sources,
    })
}
pub(super) fn valid_receipt(
    action: &GeneratorAction,
    receipt: &bace_simulation::GeneratorItemAdmission,
) -> bool {
    let GeneratorAction::AdmitMixedTrees { key, roots } = action else {
        return false;
    };
    let accepted: BTreeSet<_> = receipt.roots.iter().copied().collect();
    let failed: BTreeSet<_> = receipt.failed_roots.iter().copied().collect();
    let ids: BTreeSet<_> = receipt.entities.iter().copied().collect();
    if key != &receipt.key
        || accepted.len() != receipt.roots.len()
        || failed.len() != receipt.failed_roots.len()
        || ids.len() != receipt.entities.len()
        || !accepted.is_disjoint(&failed)
        || accepted.union(&failed).copied().collect::<BTreeSet<_>>()
            != roots.iter().map(Root::entity).collect()
    {
        return false;
    }
    let mut expected = BTreeSet::new();
    for root in roots {
        if !accepted.contains(&root.entity()) {
            continue;
        }
        match root {
            Root::Item { items, .. } => expected.extend(items.iter().map(|i| i.id)),
            Root::Creature { root, loadout, .. } => {
                expected.insert(*root);
                expected.extend(loadout.items.iter().map(|i| i.id));
            }
        }
    }
    ids == expected
}
