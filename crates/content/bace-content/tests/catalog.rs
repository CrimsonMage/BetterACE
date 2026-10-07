use bace_content::{Catalog, ContentLimits, Property, SparseProperties, WeenieTemplate};
use std::sync::Arc;

fn item(id: u32, class: &str, kind: u32, name: &str) -> WeenieTemplate {
    WeenieTemplate {
        schema_version: 1,
        weenie_id: id,
        class_name: class.into(),
        weenie_type: kind,
        last_modified: None,
        properties: SparseProperties {
            strings: vec![Property {
                id: 1,
                value: name.into(),
            }],
            ..Default::default()
        },
    }
}

#[test]
fn missing_then_live_insert_and_atomic_index_replacement_preserves_old_entities() {
    let mut catalog = Catalog::default();
    let before = catalog.snapshot();
    assert!(before.get(50).is_none());
    let publication = catalog
        .prepare(
            1,
            vec![item(50, "old", 1, "Old name")],
            ContentLimits::default(),
        )
        .unwrap();
    assert!(catalog.snapshot().get(50).is_none());
    catalog.publish(publication).unwrap();
    let existing_entity_template = catalog.snapshot().get(50).unwrap();
    let publication = catalog
        .prepare(
            2,
            vec![item(50, "new", 2, "New name")],
            ContentLimits::default(),
        )
        .unwrap();
    let after = catalog.publish(publication).unwrap();
    assert!(before.get(50).is_none());
    assert_eq!(existing_entity_template.class_name, "old");
    assert_eq!(after.get(50).unwrap().class_name, "new");
    assert!(after.by_class_name("old").is_none());
    assert!(after.by_type(1).is_empty());
    assert!(after.by_display_name("Old name").is_empty());
    assert_eq!(after.by_class_name("new").unwrap().weenie_id, 50);
    assert_eq!(after.by_type(2), vec![50]);
    assert_eq!(after.by_display_name("New name"), vec![50]);
}

#[test]
fn invalid_batch_and_stale_preparation_cannot_damage_active_generation() {
    let mut catalog = Catalog::default();
    let initial = catalog
        .prepare(1, vec![item(1, "one", 1, "one")], ContentLimits::default())
        .unwrap();
    catalog.publish(initial).unwrap();
    let before = catalog.snapshot();
    let invalid = catalog.prepare(2, vec![item(2, "one", 1, "two")], ContentLimits::default());
    assert!(invalid.is_err());
    assert!(Arc::ptr_eq(&before, &catalog.snapshot()));
    let stale = catalog
        .prepare(2, vec![item(2, "two", 1, "two")], ContentLimits::default())
        .unwrap();
    let fresh = catalog
        .prepare(
            3,
            vec![item(3, "three", 1, "three")],
            ContentLimits::default(),
        )
        .unwrap();
    catalog.publish(fresh).unwrap();
    assert!(catalog.publish(stale).is_err());
    assert!(catalog.snapshot().get(2).is_none());
    assert!(catalog.snapshot().get(3).is_some());
}

#[test]
fn duplicate_ids_properties_and_nonfinite_are_rejected() {
    let mut invalid = item(1, "one", 1, "name");
    invalid.properties.strings.push(Property {
        id: 1,
        value: "duplicate".into(),
    });
    assert!(invalid.validate(ContentLimits::default()).is_err());
    invalid.properties.strings.pop();
    invalid.properties.floats.push(Property {
        id: 1,
        value: f64::NAN,
    });
    assert!(invalid.validate(ContentLimits::default()).is_err());
    let catalog = Catalog::default();
    assert!(
        catalog
            .prepare(
                1,
                vec![item(1, "a", 1, "a"), item(1, "b", 1, "b")],
                ContentLimits::default()
            )
            .is_err()
    );
}
