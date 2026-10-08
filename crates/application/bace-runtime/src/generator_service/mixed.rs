//! Cold source-order forest preparation, including creature-valued wielded loot.
use super::*;
use bace_content::WeenieV1;
use bace_gameplay_api::GeneratorDestination;
use bace_simulation::PreparedMixedGeneratorRoot as Root;
use materialization::{Materialized, Ready};
use std::collections::BTreeSet;
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
            ) && (!source.properties.generators.is_empty()
                || gear.iter().any(|item| {
                    crate::generator_preparation::is_creature_template(item.source.weenie_type)
                        || !item.source.properties.generators.is_empty()
                }))
            {
                return Err("contained Creature nested construction/generators require their lifecycle companion".into());
            }
            Materialized::Creature {
                source: Arc::new(source),
                gear,
            }
        } else {
            let items = bace_loot::materialize_container_tree(source, &region.catalog.templates)
                .map_err(|e| format!("generator container contents: {e:?}"))?;
            for item in &items {
                if !crate::generator_preparation::is_creature_template(item.source.weenie_type) {
                    continue;
                }
                let source = &item.source;
                if !matches!(
                    request.intent.destination,
                    GeneratorDestination::Contain { .. }
                ) || !matches!(source.weenie_type, 10 | 15)
                    || item.parent_index.is_none()
                    || !source.properties.create_list.is_empty()
                    || !source.properties.generators.is_empty()
                    || source
                        .properties
                        .data_ids
                        .iter()
                        .any(|p| matches!(p.id, 32 | 33) && p.value != 0)
                {
                    return Err(
                        "nested creature requires its complete subtype/loadout owner".into(),
                    );
                }
            }
            Materialized::Items(items)
        };
        count = count
            .checked_add(tree.count())
            .filter(|n| *n <= 1024)
            .ok_or("generator forest identity capacity")?;
        forest.push(tree);
    }
    let nested_creature = forest.iter().any(|tree| match tree {
        Materialized::Items(items) => items.iter().any(|item| {
            item.parent_index.is_some()
                && crate::generator_preparation::is_creature_template(item.source.weenie_type)
        }),
        _ => false,
    });
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
