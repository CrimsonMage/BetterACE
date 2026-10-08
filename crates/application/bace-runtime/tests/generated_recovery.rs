use bace_content::{Position, Property, WeenieV1};
use bace_persistence::{DurableItemPlace, InventoryLoadLimits, LocatedSnapshot, StoredAggregate};
use bace_runtime::generated_recovery::*;
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, ItemSaveV2};
use bace_storage_codec::{
    FrozenConstructedChildV1, FrozenCreatureConstructionV1, FrozenGeneratorConstructionOriginV1,
    ItemSaveV4,
};
fn world() -> ItemPlacementV2 {
    ItemPlacementV2::World(Position {
        obj_cell_id: 0x01010001,
        position_x: 1.,
        position_y: 2.,
        position_z: 3.,
        rotation_w: 1.,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.,
    })
}
fn snapshot(id: u32, parent: Option<u32>, generator: Option<u32>, depth: u16) -> LocatedSnapshot {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "stored_world_item".into(),
        weenie_type: 51,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.ints.push(Property { id: 12, value: 7 });
    state.properties.strings.push(Property {
        id: 16,
        value: "source metadata retained".into(),
    });
    if let Some(generator) = generator {
        state.properties.instance_ids.push(Property {
            id: 6,
            value: generator,
        });
    }
    let (placement, relational) = if let Some(container) = parent {
        (
            ItemPlacementV2::Contained {
                container,
                slot: 0,
                pack_slot: false,
                equipped: 0,
            },
            DurableItemPlace::Contained {
                container,
                slot: 0,
                pack_slot: false,
                equipped: 0,
            },
        )
    } else {
        (world(), DurableItemPlace::World { cell: 0x01010001 })
    };
    let saved = bace_storage_codec::ItemSaveV4 {
        previous: bace_storage_codec::ItemSaveV3 {
            previous: ItemSaveV2 {
                entity: EntitySaveV1 {
                    object_id: id,
                    template_revision: 9,
                    mutation_revision: 2,
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
        placement: relational,
        depth,
    }
}
#[test]
fn durable_constructed_creature_and_gear_restore_before_stale_world_remnant() {
    let root = snapshot(100, None, None, 0);
    let mut creature = snapshot(101, Some(100), Some(7), 1);
    let mut saved = ItemSaveV4::decode(&creature.aggregate.bytes).unwrap();
    saved.entity.state.weenie_type = 10;
    saved.construction = Some(FrozenCreatureConstructionV1 {
        weenie_type: 10,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: 7,
            incarnation: 3,
            content_revision: 4,
            profile: 5,
            occurrence: 1,
            random_identity: [3; 16],
            random_key_version: 1,
        },
        equipment_order: vec![102],
        death_roster: vec![FrozenConstructedChildV1 {
            entity: 102,
            parent: None,
        }],
    });
    creature.aggregate.bytes = saved.encode().unwrap();
    let mut gear = snapshot(102, Some(101), Some(7), 2);
    gear.placement = DurableItemPlace::Contained {
        container: 101,
        slot: 0,
        pack_slot: false,
        equipped: 1,
    };
    let mut saved_gear = ItemSaveV4::decode(&gear.aggregate.bytes).unwrap();
    saved_gear.placement = ItemPlacementV2::Contained {
        container: 101,
        slot: 0,
        pack_slot: false,
        equipped: 1,
    };
    gear.aggregate.bytes = saved_gear.encode().unwrap();
    let stale = snapshot(200, None, Some(8), 0);
    let recovered = classify_generated_world_trees(
        vec![gear, stale, creature, root],
        44,
        InventoryLoadLimits::default(),
    )
    .unwrap();
    assert_eq!(
        recovered
            .restore_candidates
            .iter()
            .map(|row| row.aggregate.object_id)
            .collect::<Vec<_>>(),
        vec![100, 101, 102]
    );
    assert_eq!(recovered.held_for_retirement.len(), 1);
    assert_eq!(recovered.held_for_retirement[0].root, 200);
}
#[test]
fn partial_generated_remainder_is_held_whole_while_player_drop_is_restore_candidate() {
    let remainder = snapshot(100, None, Some(10), 0);
    let child = snapshot(101, Some(100), None, 1);
    let grandchild = snapshot(102, Some(101), Some(11), 2);
    let ordinary = snapshot(200, None, None, 0);
    let ordinary_child = snapshot(201, Some(200), None, 1);
    let expected = [remainder.clone(), child.clone(), grandchild.clone()];
    let result = classify_generated_world_trees(
        vec![ordinary_child, grandchild, remainder, ordinary, child],
        44,
        InventoryLoadLimits::default(),
    )
    .unwrap();
    assert_eq!(
        result
            .restore_candidates
            .iter()
            .map(|s| s.aggregate.object_id)
            .collect::<Vec<_>>(),
        vec![200, 201]
    );
    assert_eq!(result.held_for_retirement.len(), 1);
    let held = &result.held_for_retirement[0];
    assert_eq!(held.root, 100);
    assert_eq!(held.world_epoch, 44);
    assert_eq!(held.generator_links, vec![(100, 10), (102, 11)]);
    assert_eq!(held.snapshots.len(), 3);
    for (actual, before) in held.snapshots.iter().zip(expected) {
        assert_eq!(actual.aggregate.bytes, before.aggregate.bytes);
        assert_eq!(
            actual.aggregate.persisted_version,
            before.aggregate.persisted_version
        );
        assert_eq!(actual.placement, before.placement);
    }
}
#[test]
fn linked_descendant_holds_its_ordinary_root_instead_of_orphaning_items() {
    let result = classify_generated_world_trees(
        vec![
            snapshot(100, None, None, 0),
            snapshot(101, Some(100), Some(10), 1),
        ],
        44,
        InventoryLoadLimits::default(),
    )
    .unwrap();
    assert!(result.restore_candidates.is_empty());
    assert_eq!(result.held_for_retirement[0].root, 100);
    assert_eq!(
        result.held_for_retirement[0].generator_links,
        vec![(101, 10)]
    );
    assert_eq!(result.held_for_retirement[0].snapshots.len(), 2);
}
#[test]
fn invalid_epoch_bounds_placement_and_ancestry_return_every_original_snapshot() {
    let valid = vec![
        snapshot(100, None, Some(10), 0),
        snapshot(101, Some(100), None, 1),
    ];
    for (epoch, limits) in [
        (0, InventoryLoadLimits::default()),
        (
            44,
            InventoryLoadLimits {
                max_items: 1,
                ..InventoryLoadLimits::default()
            },
        ),
        (
            44,
            InventoryLoadLimits {
                max_total_bytes: 1,
                ..InventoryLoadLimits::default()
            },
        ),
    ] {
        let (_, returned) =
            classify_generated_world_trees(valid.clone(), epoch, limits).unwrap_err();
        assert_eq!(returned.len(), 2);
        assert_eq!(returned[0].aggregate.bytes, valid[0].aggregate.bytes);
    }
    let mut mismatched = valid.clone();
    mismatched[0].aggregate.object_id = 99;
    assert_eq!(
        classify_generated_world_trees(mismatched, 44, InventoryLoadLimits::default())
            .unwrap_err()
            .0,
        GeneratedRecoveryError::Identity
    );
    let mut depth = valid.clone();
    depth[1].depth = 2;
    assert_eq!(
        classify_generated_world_trees(depth, 44, InventoryLoadLimits::default())
            .unwrap_err()
            .0,
        GeneratedRecoveryError::Ancestry
    );
    let missing_parent = vec![snapshot(101, Some(999), None, 1)];
    assert_eq!(
        classify_generated_world_trees(missing_parent, 44, InventoryLoadLimits::default())
            .unwrap_err()
            .0,
        GeneratedRecoveryError::Ancestry
    );
    let cycle = vec![
        snapshot(100, Some(101), Some(10), 1),
        snapshot(101, Some(100), None, 2),
    ];
    assert!(classify_generated_world_trees(cycle, 44, InventoryLoadLimits::default()).is_err());
    let mut placement = valid;
    placement[0].placement = DurableItemPlace::World { cell: 0x02020001 };
    assert_eq!(
        classify_generated_world_trees(placement, 44, InventoryLoadLimits::default())
            .unwrap_err()
            .0,
        GeneratedRecoveryError::Placement
    );
}
