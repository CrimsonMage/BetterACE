use bace_content::{CreateListEntry, Property, WeenieV1};
use bace_gameplay_api::*;
use bace_runtime::generator_equipment::materialize_creature_equipment;
use bace_types::EntityId;
use std::{collections::BTreeMap, sync::Arc};
fn intent() -> GeneratorSpawnIntent {
    GeneratorSpawnIntent {
        key: GeneratorSpawnKey {
            generator: GeneratorIdentity {
                entity: EntityId(1),
                incarnation: 1,
                content_revision: 1,
                random_identity: [1; 16],
            },
            profile_id: 1,
            occurrence: 1,
        },
        profile: GeneratorProfile {
            id: 1,
            probability: 1.,
            weenie_class_id: 100,
            delay: None,
            init_create: 1,
            max_create: 1,
            when_create: 1,
            where_create: 0,
            stack_size: None,
            palette_id: None,
            shade: None,
            position: Default::default(),
        },
        destination: GeneratorDestination::Contain {
            container: EntityId(1),
        },
        first_spawn: true,
        due_tick: 0,
        random_identity: [2; 16],
        random_key_version: 1,
    }
}
fn item(id: u32, kind: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("template{id}"),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    }
}
fn setup() -> (WeenieV1, BTreeMap<u32, Arc<WeenieV1>>) {
    let mut source = item(100, 10);
    source.properties.ints = vec![Property { id: 6, value: 10 }];
    let mut templates = BTreeMap::new();
    for id in 200..205 {
        source.properties.create_list.push(CreateListEntry {
            database_record_id: id,
            destination_type: 10,
            weenie_class_id: id,
            stack_size: 1,
            palette: 0,
            shade: 0.,
            try_to_bond: false,
        });
        let mut weapon = item(id, 6);
        weapon.properties.ints = vec![
            Property {
                id: 9,
                value: 0x100000,
            },
            Property { id: 46, value: 2 },
            Property { id: 48, value: 45 },
        ];
        templates.insert(id, Arc::new(weapon));
    }
    (source, templates)
}
#[test]
fn retry_reconstructs_equipment_and_preserves_unselected_items() {
    let (source, templates) = setup();
    let root = bace_random::RandomRoot::new([7; 32], 1).unwrap();
    let a = materialize_creature_equipment(&source, &templates, &[], &root, &intent()).unwrap();
    let b = materialize_creature_equipment(&source, &templates, &[], &root, &intent()).unwrap();
    let project = |v: Vec<bace_loot::PreparedCreatureEquipment>| {
        v.into_iter()
            .map(|i| {
                (
                    i.source.weenie_id,
                    i.wielded_location,
                    i.death_drop,
                    i.parent_index,
                    i.inventory_slot,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(project(a.clone()), project(b));
    assert_eq!(a.len(), 5);
    assert_eq!(a.iter().filter(|i| i.wielded_location != 0).count(), 1);
}
#[test]
fn missing_requested_treasure_tables_never_produce_equipment_success() {
    let (mut source, templates) = setup();
    let root = bace_random::RandomRoot::new([7; 32], 1).unwrap();
    for property in [32, 33] {
        source.properties.data_ids = vec![Property {
            id: property,
            value: 20,
        }];
        assert!(
            materialize_creature_equipment(&source, &templates, &[], &root, &intent()).is_err()
        );
    }
}

#[test]
fn static_human_wield_clothing_survives_negative_capacity_without_changing_source() {
    let root = bace_random::RandomRoot::new([7; 32], 1).unwrap();
    let mut source = item(30997, 12);
    source.properties.ints.push(Property { id: 6, value: -1 });
    source.properties.ints.push(Property { id: 7, value: -1 });
    source.properties.create_list.push(CreateListEntry {
        database_record_id: 1,
        destination_type: 2,
        weenie_class_id: 118,
        stack_size: 1,
        palette: 0,
        shade: 0.,
        try_to_bond: false,
    });
    let mut shirt = item(118, 2);
    shirt.properties.ints = vec![
        Property {
            id: 9,
            value: 0x100,
        },
        Property { id: 4, value: 6 },
    ];
    let templates = [(118, Arc::new(shirt))].into_iter().collect();
    let gear = materialize_creature_equipment(&source, &templates, &[], &root, &intent()).unwrap();
    assert_eq!(gear.len(), 1);
    assert_eq!(gear[0].source.weenie_id, 118);
    assert_ne!(gear[0].wielded_location, 0);
    assert_eq!(source.properties.ints[0].value, -1);

    source.weenie_type = 10;
    source.weenie_id = 30991; // Pinned Society Greeter is a Creature, not a Shop.
    let greeter =
        materialize_creature_equipment(&source, &templates, &[], &root, &intent()).unwrap();
    assert_eq!(greeter.len(), 1);
    assert_ne!(greeter[0].wielded_location, 0);
    assert_eq!(source.properties.ints[0].value, -1);
}
