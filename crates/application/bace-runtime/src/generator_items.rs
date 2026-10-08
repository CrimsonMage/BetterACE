//! Immutable generator item materialization on bounded asset-preparation work.
//! Placement remains an explicit simulation command; these are not accepted poses.
use bace_content::{Property, WeenieV1};
use bace_gameplay_api::{GeneratorDestination, GeneratorSpawnIntent};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_simulation::{GeneratorAction, GeneratorHostRequest};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

pub fn materialize_generator_items(
    intent: &GeneratorSpawnIntent,
    template: Option<&WeenieV1>,
    treasure: Option<&crate::generator_treasure::PreparedGeneratorTreasure>,
    assets: &bace_loot::TreasureAssets,
    root: &bace_random::RandomRoot,
) -> Result<Vec<WeenieV1>, String> {
    if intent.profile.where_create & 0x40 != 0 {
        let table = treasure.ok_or("missing prepared generator treasure")?;
        let mut random = bace_spawning::generator_event_stream(root, intent, 0)
            .map_err(|e| format!("generator random: {e:?}"))?;
        return table
            .generate(&mut random)
            .map_err(|e| format!("generator treasure: {e:?}"));
    }
    let mut item = template
        .filter(|t| t.weenie_id == intent.profile.weenie_class_id)
        .ok_or("generator template mismatch")?
        .clone();
    if let Some(palette) = intent.profile.palette_id.filter(|p| *p > 0) {
        set(
            &mut item.properties.ints,
            3,
            i32::try_from(palette).map_err(|_| "generator palette overflow")?,
        );
    }
    if let Some(shade) = intent.profile.shade.filter(|s| *s > 0.0) {
        set(&mut item.properties.floats, 12, f64::from(shade));
    }
    if (intent.profile.palette_id.is_some_and(|p| p > 0)
        || intent.profile.shade.is_some_and(|s| s > 0.0))
        && let Some(clothing) = item.properties.data_ids.iter().find(|p| p.id == 7)
    {
        let palettes = assets
            .clothing_palettes
            .get(&clothing.value)
            .ok_or("missing generator clothing asset")?;
        let setup = item
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 1)
            .map_or(0, |p| p.value);
        let applicable = assets
            .clothing_setups
            .get(&clothing.value)
            .is_some_and(|setups| setups.contains(&setup));
        let ignore = item.properties.bools.iter().any(|p| p.id == 84 && p.value);
        if applicable && !ignore {
            let palette = item
                .properties
                .ints
                .iter()
                .find(|p| p.id == 3)
                .map_or(0, |p| p.value as u32);
            let icon = palettes.get(&palette).or_else(|| {
                assets
                    .clothing_order
                    .get(&clothing.value)
                    .and_then(|order| order.first())
                    .and_then(|first| palettes.get(first))
            });
            if let Some(icon) = icon.filter(|icon| **icon > 0) {
                set(&mut item.properties.data_ids, 8, *icon);
            }
        }
    }
    if let Some(stack) = intent.profile.stack_size.filter(|s| *s > 0) {
        bace_loot::set_treasure_stack(&mut item, stack)
            .map_err(|e| format!("generator stack: {e:?}"))?;
    }
    Ok(vec![item])
}
/// IDs come from the database allocator through the generator request reservation.
/// Contain starts after the current accepted main/backpack slot counts.
pub fn prepare_generated_item_command(
    request: &GeneratorHostRequest,
    materialized: &[WeenieV1],
    shapes: &BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
    next_slots: (u32, u32),
) -> Result<GeneratorAction, String> {
    if materialized.len() != request.entities.len()
        || materialized.is_empty()
        || materialized.len() > 1024
    {
        return Err("generator materialization count mismatch".into());
    }
    if let GeneratorDestination::Shop { .. } = request.intent.destination {
        return Ok(GeneratorAction::AdmitStock {
            key: request.intent.key,
            items: request
                .entities
                .iter()
                .copied()
                .zip(materialized.iter().cloned().map(Arc::new))
                .collect(),
        });
    }
    let mut items = Vec::with_capacity(materialized.len());
    let mut containers = Vec::new();
    let mut physical = Vec::new();
    let (mut main, mut pack) = next_slots;
    let mut seen = std::collections::BTreeSet::new();
    for (id, template) in request.entities.iter().copied().zip(materialized) {
        if !seen.insert(id) {
            return Err("duplicate reserved generator ID".into());
        }
        let pack_slot = template
            .properties
            .bools
            .iter()
            .any(|p| p.id == 81 && p.value);
        let place = match request.intent.destination {
            GeneratorDestination::Contain { container } => {
                let slot = if pack_slot {
                    let v = pack;
                    pack = pack.checked_add(1).ok_or("generator slot overflow")?;
                    v
                } else {
                    let v = main;
                    main = main.checked_add(1).ok_or("generator slot overflow")?;
                    v
                };
                ItemPlace::Contained {
                    container,
                    slot,
                    equipped: 0,
                }
            }
            _ => {
                physical.push(
                    shapes
                        .get(&template.weenie_id)
                        .ok_or("missing generated item collision shape")?
                        .clone(),
                );
                ItemPlace::World
            }
        };
        let (item, container) = prepare_inventory_item(template, id, 1, place)?;
        items.push(item);
        containers.extend(container);
    }
    Ok(GeneratorAction::AdmitItems {
        key: request.intent.key,
        items,
        containers,
        shapes: (!physical.is_empty()).then_some(physical),
    })
}
fn set<T>(properties: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(p) = properties.iter_mut().find(|p| p.id == id) {
        p.value = value;
    } else {
        properties.push(Property { id, value });
        properties.sort_by_key(|p| p.id);
    }
}

/// Shared cold projection for generated and persisted item properties.
pub fn prepare_inventory_item(
    template: &WeenieV1,
    id: bace_types::EntityId,
    revision: u64,
    place: ItemPlace,
) -> Result<(InventoryItem, Option<InventoryContainer>), String> {
    let int = |property| {
        template
            .properties
            .ints
            .iter()
            .find(|p| p.id == property)
            .map(|p| p.value)
    };
    let positive = |property, default| {
        u32::try_from(int(property).unwrap_or(default))
            .map_err(|_| "negative generator item value".to_string())
    };
    let is_container = crate::generator_preparation::is_container_template(template.weenie_type);
    let is_creature = crate::generator_preparation::is_creature_template(template.weenie_type);
    let pack_slot = template
        .properties
        .bools
        .iter()
        .any(|p| p.id == 81 && p.value);
    let unique = match int(279).unwrap_or(0) {
        0 => false,
        1 => true,
        _ => {
            return Err(
                "generated item unique limits above one require inventory count-limit support"
                    .into(),
            );
        }
    };
    let stackable = bace_loot::is_stackable(template.weenie_type);
    // Keep intrinsic-quality protection, but quantity and accepted placement
    // do not distinguish otherwise compatible stacks.
    let mut stack_identity = template.clone();
    stack_identity.class_name = "stack_identity".into();
    stack_identity.last_modified = None;
    stack_identity.properties.authoring_metadata = None;
    stack_identity
        .properties
        .instance_ids
        .retain(|p| ![1, 2, 3, 6].contains(&p.id));
    stack_identity.properties.positions.retain(|p| p.id != 1);
    stack_identity
        .properties
        .ints
        .retain(|p| ![10, 53].contains(&p.id) && (!stackable || ![5, 12, 19].contains(&p.id)));
    let bytes = bace_content_tools::compile_template(&stack_identity).map_err(|e| e.to_string())?;
    let hash = Sha256::digest(&bytes);
    let item = InventoryItem {
        structure: int(92)
            .map(u32::try_from)
            .transpose()
            .map_err(|_| "negative structure")?,
        id,
        revision,
        template: template.weenie_id,
        stack_key: u64::from_le_bytes(hash[..8].try_into().map_err(|_| "stack hash")?),
        place,
        stack: positive(12, 1)?,
        maximum_stack: positive(11, 1)?,
        unit_burden: positive(13, if stackable { 0 } else { int(5).unwrap_or(0) })?,
        unit_value: positive(15, if stackable { 0 } else { int(19).unwrap_or(0) })?,
        pack_slot,
        is_container,
        attuned: int(114).unwrap_or(0) != 0,
        trade_reserved: false,
        active_pet: false,
        unique,
        quest_allowed: true,
        valid_wield: positive(9, 0)?,
        incompatible_wield: 0,
        wield_requirements_met: false,
    };
    let container = if is_container {
        let creature_capacity = |property| match int(property).unwrap_or(0) {
            -1 => Ok(0),
            value => u32::try_from(value).map_err(|_| "invalid creature container capacity"),
        };
        Some(InventoryContainer {
            id,
            revision,
            root_owner: None,
            slots: if is_creature {
                creature_capacity(6)?
            } else {
                positive(6, 0)?
            },
            pack_slots: if is_creature {
                creature_capacity(7)?
            } else {
                positive(7, 0)?
            },
            burden_limit: if is_creature {
                u64::MAX
            } else {
                u64::from(positive(96, i32::MAX)?)
            },
            accessible: !is_creature,
            open: false,
            generation: 1,
        })
    } else {
        None
    };
    Ok((item, container))
}
