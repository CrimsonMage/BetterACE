use super::*;
use bace_content::{Position, Property, WeenieV1};
use bace_persistence::StoredAggregate;
use bace_storage_codec::{
    EntitySaveV1, FrozenCreatureConstructionV1, FrozenGeneratorConstructionOriginV1,
};
fn saved(weenie_type: u32) -> ItemSaveV4 {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "restore_guard".into(),
        weenie_type,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.strings.push(Property {
        id: 16,
        value: "preserved metadata".into(),
    });
    ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity: EntitySaveV1 {
            object_id: 123,
            template_revision: 8,
            mutation_revision: 9,
            state,
        },
        placement: ItemPlacementV2::World(Position {
            obj_cell_id: 0x12340001,
            position_x: 1.,
            position_y: 2.,
            position_z: 3.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        }),
    })
    .unwrap()
}
fn row(saved: &ItemSaveV4, legacy: bool) -> LocatedSnapshot {
    LocatedSnapshot {
        aggregate: StoredAggregate {
            object_id: 123,
            persisted_version: 4,
            bytes: if legacy {
                saved.previous.encode().unwrap()
            } else {
                saved.encode().unwrap()
            },
        },
        placement: crate::game_inventory::durable(&saved.placement),
        depth: 0,
    }
}
#[test]
fn ordinary_item_keeps_exact_source_and_pose() {
    let saved = saved(1);
    let row = row(&saved, false);
    let before = row.clone();
    assert_eq!(decode_for_restore(&row).unwrap().item, saved);
    assert_eq!(row.aggregate.bytes, before.aggregate.bytes);
    assert_eq!(row.placement, before.placement);
}
#[test]
fn v5_origin_survives_world_cold_decode_without_inferring_legacy_origin() {
    let saved = saved(1);
    let v5 = ItemSaveV5 {
        previous: saved.clone(),
        source_destination: Some(9),
    };
    let snapshot = LocatedSnapshot {
        aggregate: StoredAggregate {
            object_id: 123,
            persisted_version: 4,
            bytes: v5.encode().unwrap(),
        },
        placement: crate::game_inventory::durable(&saved.placement),
        depth: 0,
    };
    let decoded = decode_for_restore(&snapshot).unwrap();
    assert_eq!(decoded.item, saved);
    assert_eq!(decoded.source_destination, Some(9));
    assert_eq!(snapshot.aggregate.bytes, v5.encode().unwrap());
    assert_eq!(
        decode_for_restore(&row(&saved, false))
            .unwrap()
            .source_destination,
        None
    );
}
#[test]
fn nested_constructed_source_parent_is_a_saved_ancestor_of_its_machine() {
    let mut child = saved(10);
    child.previous.previous.placement = ItemPlacementV2::Contained {
        container: 200,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    };
    child
        .entity
        .state
        .properties
        .instance_ids
        .push(Property { id: 6, value: 200 });
    let mut parent = saved(21);
    parent.previous.previous.entity.object_id = 200;
    parent.previous.previous.placement = ItemPlacementV2::Contained {
        container: 1,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    };
    let saved = BTreeMap::from([(EntityId(200), parent)]);
    assert!(constructed_source_parent_valid(&child, &saved, 7));
    assert!(!constructed_source_parent_valid(
        &child,
        &BTreeMap::new(),
        7
    ));
    child.entity.state.properties.instance_ids[0].value = 999;
    assert!(!constructed_source_parent_valid(&child, &saved, 7));
}
#[test]
fn companion_and_legacy_creature_are_held_without_generic_admission_or_lost_bytes() {
    let mut saved = saved(10);
    let legacy = row(&saved, true);
    saved.construction = Some(FrozenCreatureConstructionV1 {
        weenie_type: 10,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: 7,
            incarnation: 1,
            content_revision: 2,
            profile: 0,
            occurrence: 0,
            random_identity: [3; 16],
            random_key_version: 1,
        },
        equipment_order: vec![],
        death_roster: vec![],
    });
    for row in [legacy, row(&saved, false)] {
        let original = row.clone();
        assert!(decode_for_restore(&row).is_err());
        assert_eq!(row.aggregate.bytes, original.aggregate.bytes);
        assert_eq!(row.placement, original.placement);
        assert_eq!(
            decode(&row).unwrap().item.entity.state.weenie_type,
            10,
            "checked snapshots remain readable for reconciliation"
        );
    }
}
#[test]
fn contained_v4_creature_reaches_constructed_restore_with_exact_source_bytes() {
    let mut value = saved(10);
    value.previous.previous.placement = ItemPlacementV2::Contained {
        container: 1,
        slot: 2,
        pack_slot: false,
        equipped: 0,
    };
    value
        .entity
        .state
        .properties
        .instance_ids
        .push(Property { id: 6, value: 7 });
    value.construction = Some(FrozenCreatureConstructionV1 {
        weenie_type: 10,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: 7,
            incarnation: 1,
            content_revision: 2,
            profile: 0,
            occurrence: 1,
            random_identity: [3; 16],
            random_key_version: 1,
        },
        equipment_order: vec![],
        death_roster: vec![],
    });
    let original = row(&value, false);
    let decoded = decode_for_restore(&original).unwrap();
    assert_eq!(decoded.item, value);
    assert_eq!(original.aggregate.bytes, value.encode().unwrap());
}
