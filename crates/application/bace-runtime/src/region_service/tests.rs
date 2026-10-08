use super::*;
use bace_content::{Position, Property, WeenieV1};
use bace_persistence::{DurableItemPlace, StoredAggregate};
use bace_storage_codec::{
    CorpseSaveV1, CorpseSaveV2, CorpseSaveV3, CorpseSaveV4, CorpseSaveV5, EntitySaveV1,
    ItemPlacementV2, ItemSaveV2, ItemSaveV4,
};
fn source(id: u32, parent: Option<u32>, generated: bool) -> LocatedSnapshot {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "retained_world_item".into(),
        weenie_type: 51,
        last_modified: None,
        properties: Default::default(),
    };
    if generated {
        state
            .properties
            .instance_ids
            .push(Property { id: 6, value: 50 });
    }
    state.properties.strings.push(Property {
        id: 16,
        value: "exact retained metadata".into(),
    });
    let placement = parent.map_or_else(
        || {
            ItemPlacementV2::World(Position {
                obj_cell_id: 0x12340001,
                position_x: 1.,
                position_y: 2.,
                position_z: 3.,
                rotation_w: 1.,
                rotation_x: 0.,
                rotation_y: 0.,
                rotation_z: 0.,
            })
        },
        |container| ItemPlacementV2::Contained {
            container,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        },
    );
    let saved = bace_storage_codec::ItemSaveV4 {
        previous: bace_storage_codec::ItemSaveV3 {
            previous: ItemSaveV2 {
                entity: EntitySaveV1 {
                    object_id: id,
                    template_revision: 9,
                    mutation_revision: 7,
                    state,
                },
                placement,
            },
            enchantments: vec![],
        },
        construction: None,
    };
    LocatedSnapshot {
        aggregate: StoredAggregate {
            object_id: id,
            persisted_version: 3,
            bytes: saved.encode().unwrap(),
        },
        placement: crate::game_inventory::durable(&saved.placement),
        depth: u16::from(parent.is_some()),
    }
}
#[test]
fn stale_generator_reconciliation_freezes_all_descendants_with_exact_versions() {
    let rows = vec![source(100, None, true), source(101, Some(100), false)];
    let original = rows.clone();
    let recovered =
        crate::generated_recovery::classify_generated_world_trees(rows, 8, limits()).unwrap();
    let frozen = reconciliation::freeze(&recovered.held_for_retirement[0]).unwrap();
    let operation = frozen.operation();
    assert_eq!(operation.operation_id, "generated-recovery:8:100");
    assert_eq!(operation.snapshots.len(), 2);
    for s in &operation.snapshots {
        assert_eq!(s.expected_version, 3);
        assert_eq!(s.mutation_revision, 8);
        let saved = bace_storage_codec::ItemSaveV5::decode_or_migrate(&s.bytes, None).unwrap();
        assert_eq!(saved.placement, ItemPlacementV2::Removed);
        assert_eq!(
            saved.entity.state.properties.strings[0].value,
            "exact retained metadata"
        );
    }
    assert!(
        operation
            .changes
            .iter()
            .all(|c| c.destination == DurableItemPlace::Removed)
    );
    assert_eq!(
        recovered.held_for_retirement[0].snapshots[0]
            .aggregate
            .bytes,
        original[0].aggregate.bytes
    );
}
#[test]
fn corpse_v4_classification_migrates_to_v5_reconciliation_without_losing_identity_or_deadline() {
    let mut root = source(100, None, true);
    let item = ItemSaveV4::decode(&root.aggregate.bytes).unwrap();
    let corpse = CorpseSaveV4 {
        previous: CorpseSaveV3 {
            previous: CorpseSaveV2 {
                corpse: CorpseSaveV1 {
                    entity: item.previous.previous.entity,
                    owner: Some(5),
                    death_operation: "native-death-aabbccdd".into(),
                    expires_at: 123456,
                },
                placement: item.previous.previous.placement,
            },
            enchantments: vec![],
        },
        source: Some(80),
        operation: Some(55),
    };
    root.aggregate.bytes = corpse.encode().unwrap();
    let recovered =
        crate::generated_recovery::classify_generated_world_trees(vec![root], 8, limits()).unwrap();
    let frozen = reconciliation::freeze(&recovered.held_for_retirement[0]).unwrap();
    let after = CorpseSaveV5::decode(&frozen.operation().snapshots[0].bytes).unwrap();
    assert_eq!(after.source, Some(80));
    assert_eq!(after.operation, Some(55));
    assert_eq!(after.corpse.expires_at, 123456);
    assert_eq!(after.corpse.death_operation, "native-death-aabbccdd");
    assert_eq!(after.placement, ItemPlacementV2::Removed);
    assert_eq!(after.access.victim, Some(80));
    assert!(!after.access.looted);
    assert!(after.access.permittees.is_empty());
}
#[test]
fn ordinary_corpse_is_a_restore_candidate_and_legacy_identity_stays_unknown() {
    let mut root = source(100, None, false);
    let item = ItemSaveV4::decode(&root.aggregate.bytes).unwrap();
    root.aggregate.bytes = CorpseSaveV3 {
        previous: CorpseSaveV2 {
            corpse: CorpseSaveV1 {
                entity: item.previous.previous.entity,
                owner: Some(5),
                death_operation: "legacy-native-event".into(),
                expires_at: 123456,
            },
            placement: item.previous.previous.placement,
        },
        enchantments: vec![],
    }
    .encode()
    .unwrap();
    let recovered =
        crate::generated_recovery::classify_generated_world_trees(vec![root], 8, limits()).unwrap();
    assert!(recovered.held_for_retirement.is_empty());
    let decoded = world_items::decode(&recovered.restore_candidates[0]).unwrap();
    let corpse = decoded.corpse.unwrap();
    assert_eq!((corpse.source, corpse.operation), (None, None));
    assert_eq!(corpse.corpse.expires_at, 123456);
}
