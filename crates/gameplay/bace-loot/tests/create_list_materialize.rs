use bace_content::{CreateListEntry, Property, SparseProperties, WeenieV1};
use bace_loot::materialize_create_list;
fn template() -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: 20,
        class_name: "source".into(),
        weenie_type: 51,
        last_modified: None,
        properties: SparseProperties {
            ints: vec![
                Property { id: 11, value: 100 },
                Property { id: 12, value: 1 },
                Property { id: 13, value: 2 },
                Property { id: 15, value: 3 },
            ],
            floats: vec![Property { id: 12, value: 0.9 }],
            ..Default::default()
        },
    }
}
#[test]
fn source_trophy_probability_never_becomes_appearance_shade() {
    let row = CreateListEntry {
        weenie_class_id: 20,
        stack_size: 4,
        palette: 7,
        destination_type: 8,
        shade: 0.2,
        ..Default::default()
    };
    let original = template();
    let item = materialize_create_list(&row, &original).unwrap();
    assert_eq!(item.properties.floats[0].value, 0.9);
    assert_eq!(
        item.properties
            .ints
            .iter()
            .find(|p| p.id == 3)
            .unwrap()
            .value,
        7
    );
    assert_eq!(
        item.properties
            .ints
            .iter()
            .find(|p| p.id == 12)
            .unwrap()
            .value,
        4
    );
    assert_eq!(
        original
            .properties
            .ints
            .iter()
            .find(|p| p.id == 12)
            .unwrap()
            .value,
        1
    );
}
#[test]
fn contain_shade_and_nonstackable_source_behavior() {
    let row = CreateListEntry {
        weenie_class_id: 20,
        stack_size: 4,
        destination_type: 1,
        shade: 0.25,
        ..Default::default()
    };
    let mut original = template();
    original.weenie_type = 1;
    original.properties.ints.clear();
    let item = materialize_create_list(&row, &original).unwrap();
    assert_eq!(item.properties.floats[0].value, 0.25);
    assert!(!item.properties.ints.iter().any(|p| p.id == 12));
}
