//! Source boundary regression for WorldObject_Description.CalculateObjDesc and
//! generator profile overrides; source pin is docs/baselines.toml.
use bace_content::{Property, WeenieV1};
use bace_gameplay_api::*;
use bace_loot::TreasureAssets;
use bace_runtime::generator_items::materialize_generator_items;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
fn intent() -> GeneratorSpawnIntent {
    GeneratorSpawnIntent {
        key: GeneratorSpawnKey {
            generator: GeneratorIdentity {
                entity: EntityId(1),
                incarnation: 1,
                content_revision: 1,
                random_identity: [1; 16],
            },
            profile_id: 0,
            occurrence: 1,
        },
        profile: GeneratorProfile {
            id: 0,
            probability: 1.0,
            weenie_class_id: 100,
            delay: None,
            init_create: 1,
            max_create: 1,
            when_create: 1,
            where_create: 0,
            stack_size: Some(3),
            palette_id: Some(999),
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

fn template() -> WeenieV1 {
    let mut item = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "generated".into(),
        weenie_type: 51,
        last_modified: None,
        properties: Default::default(),
    };
    item.properties.data_ids = vec![
        Property {
            id: 1,
            value: 0x02000001,
        },
        Property {
            id: 7,
            value: 0x10000001,
        },
        Property { id: 8, value: 50 },
    ];
    item.properties.ints = vec![
        Property { id: 11, value: 10 },
        Property { id: 13, value: 2 },
        Property { id: 15, value: 7 },
    ];
    item
}
fn assets() -> TreasureAssets {
    let mut assets = TreasureAssets::default();
    assets
        .clothing_palettes
        .insert(0x10000001, BTreeMap::from([(8, 80), (94, 940)]));
    assets.clothing_order.insert(0x10000001, vec![94, 8]);
    assets
        .clothing_setups
        .insert(0x10000001, BTreeSet::from([0x02000001]));
    assets
}
#[test]
fn source_profile_uses_authored_first_palette_and_exact_stack_totals() {
    let root = bace_random::RandomRoot::new([7; 32], 1).unwrap();
    let input = template();
    let result = materialize_generator_items(&intent(), Some(&input), None, &assets(), &root)
        .unwrap()
        .remove(0);
    assert_eq!(
        result
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 8)
            .unwrap()
            .value,
        940
    );
    for (id, value) in [(12, 3), (5, 6), (19, 21)] {
        assert_eq!(
            result
                .properties
                .ints
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .value,
            value
        );
    }
    assert_eq!(input.properties.data_ids[2].value, 50);
}
#[test]
fn source_icon_override_respects_setup_and_ignore_flag() {
    let root = bace_random::RandomRoot::new([7; 32], 1).unwrap();
    for ignore in [false, true] {
        let mut item = template();
        if ignore {
            item.properties.bools.push(Property {
                id: 84,
                value: true,
            });
        } else {
            item.properties.data_ids[0].value = 0x02000002;
        }
        let output =
            materialize_generator_items(&intent(), Some(&item), None, &assets(), &root).unwrap();
        assert_eq!(
            output[0]
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == 8)
                .unwrap()
                .value,
            50
        );
    }
    assert!(
        materialize_generator_items(
            &intent(),
            Some(&template()),
            None,
            &TreasureAssets::default(),
            &root
        )
        .is_err()
    );
}
