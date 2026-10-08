//! Generator inventory projections must retain intrinsic item distinctions while
//! allowing the quantity/placement changes performed by pinned ACE inventory.
use bace_content::{Position, Property, WeenieV1};
use bace_gameplay_api::*;
use bace_inventory::InventoryItem;
use bace_runtime::generator_items::prepare_generated_item_command;
use bace_simulation::{GeneratorAction, GeneratorHostRequest};
use bace_types::EntityId;
use std::collections::BTreeMap;
#[path = "generator_delivery/fixture.rs"]
mod generator_fixture;
fn template() -> WeenieV1 {
    let mut template = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "generated_stack".into(),
        weenie_type: 51,
        last_modified: None,
        properties: Default::default(),
    };
    template.properties.ints = [(11, 100), (12, 3), (13, 2), (15, 7), (5, 6), (19, 21)]
        .into_iter()
        .map(|(id, value)| Property { id, value })
        .collect();
    template
}
fn project(template: &WeenieV1) -> Result<InventoryItem, String> {
    let definition = generator_fixture::definition();
    let request = GeneratorHostRequest {
        landblock: 0x0101,
        next_slots: Some((0, 0)),
        intent: GeneratorSpawnIntent {
            key: GeneratorSpawnKey {
                generator: definition.identity,
                profile_id: 0,
                occurrence: 1,
            },
            profile: definition.profiles[0].clone(),
            destination: GeneratorDestination::Contain {
                container: EntityId(10),
            },
            first_spawn: true,
            due_tick: 0,
            random_identity: [2; 16],
            random_key_version: 1,
        },
        entities: vec![EntityId(0x80000b01)],
    };
    let GeneratorAction::AdmitItems { mut items, .. } = prepare_generated_item_command(
        &request,
        std::slice::from_ref(template),
        &BTreeMap::new(),
        (0, 0),
    )?
    else {
        panic!("contained item projection")
    };
    Ok(items.remove(0))
}
#[test]
fn stack_identity_ignores_quantity_runtime_placement_and_authoring_metadata() {
    let base = template();
    let expected = project(&base).unwrap().stack_key;
    let mut moved = base.clone();
    for property in &mut moved.properties.ints {
        match property.id {
            12 => property.value = 7,
            5 => property.value = 14,
            19 => property.value = 49,
            _ => {}
        }
    }
    moved
        .properties
        .ints
        .extend([Property { id: 53, value: 4 }, Property { id: 10, value: 8 }]);
    moved
        .properties
        .instance_ids
        .extend([1, 2, 3, 6].map(|id| Property {
            id,
            value: 0x80000001,
        }));
    moved.properties.positions.push(Property {
        id: 1,
        value: Position {
            obj_cell_id: 0x01010001,
            position_x: 1.,
            position_y: 2.,
            position_z: 3.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        },
    });
    moved.class_name = "authoring_rename".into();
    assert_eq!(project(&moved).unwrap().stack_key, expected);
    let mut quality_changed = base;
    quality_changed
        .properties
        .ints
        .push(Property { id: 3, value: 95 });
    assert_ne!(
        project(&quality_changed).unwrap().stack_key,
        expected,
        "intrinsic-quality hardening remains explicit"
    );
}
#[test]
fn missing_stack_units_use_source_zero_while_nonstack_items_keep_total_values() {
    let mut stack = template();
    stack
        .properties
        .ints
        .retain(|property| ![13, 15].contains(&property.id));
    let item = project(&stack).unwrap();
    assert_eq!((item.unit_burden, item.unit_value), (0, 0));
    stack.weenie_type = 1;
    stack
        .properties
        .ints
        .retain(|property| ![11, 12].contains(&property.id));
    let item = project(&stack).unwrap();
    assert_eq!((item.unit_burden, item.unit_value), (6, 21));
}
#[test]
fn unique_limit_one_is_enforced_and_unsupported_higher_limits_are_rejected() {
    let mut item = template();
    item.properties.ints.push(Property { id: 279, value: 1 });
    assert!(project(&item).unwrap().unique);
    item.properties.ints.last_mut().unwrap().value = 2;
    assert!(project(&item).is_err());
}
