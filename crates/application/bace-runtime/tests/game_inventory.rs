use bace_content::{Position, Property, WeenieV1};
use bace_inventory::*;
use bace_runtime::game_inventory::*;
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2};
use bace_types::EntityId;
use std::collections::BTreeMap;
#[test]
fn freezing_drop_requires_admitted_pose_and_preserves_unrelated_native_item_values() {
    let owner = 0x50000001;
    let id = 0x80000001;
    let before = InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: 100,
        stack_key: 10,
        place: ItemPlace::Contained {
            container: EntityId(owner),
            slot: 0,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 5,
        unit_value: 7,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    };
    let mut after = before.clone();
    after.place = ItemPlace::World;
    after.revision = 2;
    let proposal = InventoryProposal {
        changes: vec![ItemChange {
            before: Some(before),
            after,
        }],
        participants: vec![(EntityId(id), 1), (EntityId(owner), 1)],
        actor_burden: 0,
        requires_pickup_motion: true,
    };
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "frozen_test".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.instance_ids = vec![
        Property {
            id: 1,
            value: owner,
        },
        Property {
            id: 2,
            value: owner,
        },
    ];
    state.properties.int64s = vec![Property {
        id: 0xffff,
        value: i64::MIN,
    }];
    state.properties.strings = vec![Property {
        id: 999,
        value: "unknown retained property".into(),
    }];
    let items = [FrozenInventoryItem {
        corpse: None,
        construction: None,
        source_destination: Some(9),
        enchantments: vec![],
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 5,
            mutation_revision: 1,
            state,
        },
        placement: Some(ItemPlacementV2::Contained {
            container: owner,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        }),
        persisted_version: 3,
    }];
    let empty_positions = BTreeMap::new();
    let mut positions = BTreeMap::new();
    let input = |positions| InventoryFreezeInput {
        operation_id: "drop-test",
        proposal: &proposal,
        items: &items,
        other_snapshots: &[],
        leases: &[],
        storage_views: &[],
        admitted_positions: positions,
    };
    assert!(
        matches!(freeze_inventory(input(&empty_positions)),Err(InventoryFreezeError::MissingPosition(x)) if x==id)
    );
    positions.insert(
        id,
        Position {
            obj_cell_id: 0x12340001,
            position_x: 1.0,
            position_y: 2.0,
            position_z: 3.0,
            rotation_w: 1.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
        },
    );
    let op = freeze_inventory(input(&positions)).unwrap();
    let saved = bace_storage_codec::ItemSaveV5::decode(&op.snapshots[0].bytes).unwrap();
    assert_eq!(saved.source_destination, Some(9));
    assert_eq!(saved.entity.state.properties.int64s[0].value, i64::MIN);
    assert_eq!(
        saved.entity.state.properties.strings[0].value,
        "unknown retained property"
    );
    assert!(saved.entity.state.properties.instance_ids.is_empty());
    assert!(
        !saved
            .entity
            .state
            .properties
            .ints
            .iter()
            .any(|p| p.id == 12)
    );
    assert_eq!(
        saved.entity.state.properties.positions[0].value,
        positions[&id]
    );
    assert_eq!(op.snapshots[0].expected_version, 3);
    assert_eq!(saved.entity.mutation_revision, 2);
}

#[test]
fn first_generated_acquisition_uses_absent_durable_source_and_preserves_live_fences() {
    let id = 0x80000101;
    let before = InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 7,
        template: 100,
        stack_key: 1,
        place: ItemPlace::World,
        stack: 1,
        maximum_stack: 1,
        unit_burden: 5,
        unit_value: 7,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: false,
    };
    let mut after = before.clone();
    after.revision = 8;
    after.place = ItemPlace::Contained {
        container: EntityId(1),
        slot: 0,
        equipped: 0,
    };
    let proposal = InventoryProposal {
        changes: vec![ItemChange {
            before: Some(before),
            after,
        }],
        participants: vec![(EntityId(1), 1), (EntityId(id), 7)],
        actor_burden: 5,
        requires_pickup_motion: true,
    };
    let position = Position {
        obj_cell_id: 0x12340001,
        position_x: 1.0,
        position_y: 2.0,
        position_z: 3.0,
        rotation_w: 1.0,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
    };
    let mut source = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "generated".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    source.properties.instance_ids.push(Property {
        id: 6,
        value: 0x800000ff,
    });
    source.properties.strings.push(Property {
        id: 16,
        value: "source description retained".into(),
    });
    let items = vec![FrozenInventoryItem {
        corpse: None,
        construction: None,
        source_destination: Some(10),
        enchantments: vec![],
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 9,
            mutation_revision: 7,
            state: source,
        },
        placement: Some(ItemPlacementV2::World(position)),
        persisted_version: 0,
    }];
    let positions = BTreeMap::new();
    let leases = [bace_persistence::CharacterLease {
        character_id: 1,
        epoch: 1,
        state: bace_persistence::OwnershipState::Online,
    }];
    let input = |items| InventoryFreezeInput {
        operation_id: "generated-first-save",
        proposal: &proposal,
        items,
        other_snapshots: &[],
        leases: &leases,
        storage_views: &[],
        admitted_positions: &positions,
    };
    let ordinary = freeze_inventory(input(&items)).unwrap();
    assert!(ordinary.changes[0].expected.is_some());
    let generated = freeze_generated_inventory(input(&items), &[id], 4).unwrap();
    assert_eq!(generated.world_epoch, 4);
    assert_eq!(generated.inventory.changes[0].expected, None);
    assert_eq!(generated.inventory.snapshots[0].expected_version, 0);
    let saved =
        bace_storage_codec::ItemSaveV5::decode(&generated.inventory.snapshots[0].bytes).unwrap();
    assert_eq!(saved.source_destination, Some(10));
    assert_eq!(saved.entity.mutation_revision, 8);
    assert!(
        saved
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .all(|p| p.id != 6)
    );
    assert_eq!(
        saved.entity.state.properties.strings[0].value,
        "source description retained"
    );
    assert!(freeze_generated_inventory(input(&items), &[id, id], 4).is_err());
    assert!(freeze_generated_inventory(input(&items), &[id], 0).is_err());
    let mut persisted = items.clone();
    persisted[0].persisted_version = 1;
    assert!(freeze_generated_inventory(input(&persisted), &[id], 4).is_err());
    let mut stale = items.clone();
    stale[0].entity.mutation_revision = 6;
    assert!(freeze_generated_inventory(input(&stale), &[id], 4).is_err());
}
