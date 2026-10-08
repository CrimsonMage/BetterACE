//! The pin's factory switch, including defaults, is the subtype boundary.
use super::*;
#[test]
fn factory_ai_is_generic_and_vendor_remains_a_creature_with_vendor_generator_rules() {
    let mut w = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "factory_class".into(),
        weenie_type: 16,
        last_modified: None,
        properties: Default::default(),
    };
    let identity = GeneratorIdentity {
        entity: bace_types::EntityId(1),
        incarnation: 1,
        content_revision: 1,
        random_identity: [1; 16],
    };
    let location = GeneratorLocation {
        cell: 0x01010001,
        origin: [0.; 3],
        rotation: [0., 0., 0., 1.],
    };
    let def = prepare_generator(&w, identity, location, &[], Default::default()).unwrap();
    assert_eq!(def.kind, GeneratorKind::Object);
    assert!(!is_creature_template(16));
    w.weenie_type = 12;
    let def = prepare_generator(&w, identity, location, &[], Default::default()).unwrap();
    assert_eq!(def.kind, GeneratorKind::Vendor);
    assert!(is_creature_template(12));
    for ty in [10, 15, 69, 71] {
        assert!(is_creature_template(ty));
    }
}
#[test]
fn unchanged_original_factory_switch_defines_all_creature_subtypes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/factory_classes.json")).unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 75);
    for row in fixture["cases"].as_array().unwrap() {
        let id = row["id"].as_u64().unwrap() as u32;
        assert_eq!(
            is_container_template(id),
            row["container"].as_bool().unwrap(),
            "container {id}"
        );
        assert_eq!(
            is_creature_template(id),
            row["creature"].as_bool().unwrap(),
            "type {id}: {}",
            row["type"]
        );
    }
}

#[test]
fn item_capacity_does_not_turn_generic_objects_into_container_instances() {
    let mut w = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "constructor_capacity".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    w.properties
        .ints
        .push(bace_content::Property { id: 6, value: 24 });
    let (item, container) = crate::generator_items::prepare_inventory_item(
        &w,
        bace_types::EntityId(100),
        1,
        bace_inventory::ItemPlace::World,
    )
    .unwrap();
    assert!(!item.is_container);
    assert!(container.is_none());
    w.weenie_type = 21;
    w.properties.ints.clear();
    let (item, container) = crate::generator_items::prepare_inventory_item(
        &w,
        bace_types::EntityId(100),
        1,
        bace_inventory::ItemPlace::World,
    )
    .unwrap();
    assert!(item.is_container);
    assert_eq!(container.unwrap().slots, 0);
}
