use bace_content::{Position, Property, WeenieV1};
use bace_runtime::death_saves::{
    DeathFreezeInput, NoCorpseFreezeInput, freeze_native_death, freeze_native_no_corpse_death,
};
use bace_simulation::{DeathProposal, NativeDeathLoot};
use bace_storage_codec::{EntitySaveV1, ItemSaveV5};
use bace_types::EntityId;
fn entity(id: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 7,
        mutation_revision: 1,
        state: WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "treasure_snapshot".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
fn pose() -> Position {
    Position {
        obj_cell_id: 0x12340001,
        position_x: 1.0,
        position_y: 2.0,
        position_z: 3.0,
        rotation_w: 1.0,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
    }
}
#[test]
fn ace_death_freeze_preserves_complete_frozen_item_and_rejects_substitutions() {
    let mut item = entity(0x80000002);
    item.state.properties.ints = vec![
        Property {
            id: 19,
            value: 9123,
        },
        Property { id: 158, value: 5 },
    ];
    item.state.properties.floats = vec![Property {
        id: 147,
        value: 1.234,
    }];
    item.state.properties.data_ids = vec![Property {
        id: 8,
        value: 0x06000001,
    }];
    item.state.properties.strings = vec![Property {
        id: 1,
        value: "Mutated source item".into(),
    }];
    let proposal = DeathProposal {
        social: None,
        position: Some(pose()),
        no_corpse: false,
        corpse_decay_ticks: 300,
        olthoi_killer: false,
        operation: 1,
        victim: EntityId(0x80000003),
        owner: None,
        corpse_template: 100,
        drops: vec![bace_simulation::LootDrop {
            template: 100,
            stack: 1,
        }],
        experience: vec![],
        experience_state: vec![],
        native: Some(NativeDeathLoot {
            event_id: [1; 16],
            key_version: 1,
            graph_id: 1,
            graph_revision: [2; 32],
            content_generation: [3; 32],
            rare_profile_revision: None,
            generated: vec![bace_loot::LootDrop {
                template: 100,
                stack: 1,
                node: "ace:0".into(),
                mutations: vec![],
            }],
            rare: None,
            source_parents: None,
            source_items: Some(vec![item.state.clone()]),
        }),
    };
    let freeze = |items: &[EntitySaveV1]| {
        freeze_native_death(DeathFreezeInput {
            proposal: &proposal,
            corpse: entity(0x80000001),
            position: pose(),
            expires_at: 1000,
            items,
            players: &[],
            leases: &[],
        })
    };
    let operation = freeze(&[item.clone()]).unwrap();
    let saved = ItemSaveV5::decode_or_migrate(&operation.snapshots[1].bytes, None).unwrap();
    assert_eq!(saved.entity.template_revision, 7);
    assert_eq!(
        saved.entity.state.properties.floats,
        item.state.properties.floats
    );
    assert_eq!(
        saved.entity.state.properties.data_ids,
        item.state.properties.data_ids
    );
    assert_eq!(
        saved.entity.state.properties.strings,
        item.state.properties.strings
    );
    assert_eq!(
        saved
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 19)
            .unwrap()
            .value,
        9123
    );
    let mut changed = item.clone();
    changed.state.properties.ints[0].value = 1;
    assert!(freeze(&[changed]).is_err());
    assert!(freeze(&[]).is_err());
    assert_eq!(
        operation.snapshots[1].bytes,
        freeze(&[item]).unwrap().snapshots[1].bytes
    );
}
#[test]
fn corpse_transfer_preserves_nested_bag_parent_and_separate_pack_slots() {
    let mut bag = entity(0x80000002);
    bag.state.properties.ints.push(Property { id: 6, value: 2 });
    bag.state.properties.bools.push(Property {
        id: 81,
        value: true,
    });
    let child = entity(0x80000003);
    let root_item = entity(0x80000004);
    let items = vec![bag, child, root_item];
    let mut proposal = DeathProposal {
        social: None,
        position: Some(pose()),
        no_corpse: false,
        corpse_decay_ticks: 300,
        olthoi_killer: false,
        operation: 1,
        victim: EntityId(0x80000005),
        owner: None,
        corpse_template: 100,
        drops: vec![
            bace_simulation::LootDrop {
                template: 100,
                stack: 1
            };
            3
        ],
        experience: vec![],
        experience_state: vec![],
        native: Some(NativeDeathLoot {
            event_id: [1; 16],
            key_version: 1,
            graph_id: 1,
            graph_revision: [2; 32],
            content_generation: [3; 32],
            rare_profile_revision: None,
            generated: (0..3)
                .map(|i| bace_loot::LootDrop {
                    template: 100,
                    stack: 1,
                    node: format!("ace:{i}"),
                    mutations: vec![],
                })
                .collect(),
            rare: None,
            source_items: Some(items.iter().map(|i| i.state.clone()).collect()),
            source_parents: Some(vec![None, Some(0), None]),
        }),
    };
    let freeze = |proposal: &DeathProposal| {
        freeze_native_death(DeathFreezeInput {
            proposal,
            corpse: entity(0x80000001),
            position: pose(),
            expires_at: 1000,
            items: &items,
            players: &[],
            leases: &[],
        })
    };
    let operation = freeze(&proposal).unwrap();
    for (index, container, pack) in [
        (1, 0x80000001, true),
        (2, 0x80000002, false),
        (3, 0x80000001, false),
    ] {
        let saved = ItemSaveV5::decode_or_migrate(&operation.snapshots[index].bytes, None).unwrap();
        assert_eq!(
            saved.placement,
            bace_storage_codec::ItemPlacementV2::Contained {
                container,
                slot: 0,
                pack_slot: pack,
                equipped: 0
            }
        );
        assert_eq!(
            saved
                .entity
                .state
                .properties
                .instance_ids
                .iter()
                .find(|p| p.id == 1)
                .unwrap()
                .value,
            container
        );
    }
    proposal.native.as_mut().unwrap().source_parents = Some(vec![Some(1), Some(0), None]);
    assert!(freeze(&proposal).is_err());
    proposal.native.as_mut().unwrap().source_parents = Some(vec![None, None, Some(1)]);
    assert!(freeze(&proposal).is_err());
}

/// Pinned ACE Creature_Death.CreateCorpse NoCorpse branch, oracle/no_corpse.py:
/// quest roots retain the dying creature as generator and each world root gets
/// an independent copy of the death pose; contained descendants stay nested.
#[test]
fn no_corpse_world_drop_preserves_quest_root_and_nested_contents() {
    let mut quest_bag = entity(0x8000_0002);
    quest_bag
        .state
        .properties
        .ints
        .push(Property { id: 6, value: 2 });
    quest_bag.state.properties.strings.push(Property {
        id: 33,
        value: "quest".into(),
    });
    let child = entity(0x8000_0003);
    let ordinary = entity(0x8000_0004);
    let items = vec![quest_bag, child, ordinary];
    let position = Position {
        obj_cell_id: 0x1234_0001,
        position_x: 12.5,
        position_y: -3.25,
        position_z: -0.125,
        rotation_w: 1.,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.,
    };
    let proposal = DeathProposal {
        social: None,
        position: Some(position.clone()),
        no_corpse: true,
        corpse_decay_ticks: 300,
        olthoi_killer: false,
        operation: 1,
        victim: EntityId(0x8000_0005),
        owner: None,
        corpse_template: 100,
        drops: vec![
            bace_simulation::LootDrop {
                template: 100,
                stack: 1,
            };
            3
        ],
        experience: vec![],
        experience_state: vec![],
        native: Some(NativeDeathLoot {
            event_id: [4; 16],
            key_version: 1,
            graph_id: 1,
            graph_revision: [2; 32],
            content_generation: [3; 32],
            rare_profile_revision: None,
            generated: (0..3)
                .map(|i| bace_loot::LootDrop {
                    template: 100,
                    stack: 1,
                    node: format!("ace:{i}"),
                    mutations: vec![],
                })
                .collect(),
            rare: None,
            source_items: Some(items.iter().map(|i| i.state.clone()).collect()),
            source_parents: Some(vec![None, Some(0), None]),
        }),
    };
    let freeze = |proposal: &DeathProposal, items: &[EntitySaveV1], olthoi_killer| {
        freeze_native_no_corpse_death(NoCorpseFreezeInput {
            proposal,
            position: position.clone(),
            items,
            players: &[],
            leases: &[],
            olthoi_killer,
        })
    };
    let operation = freeze(&proposal, &items, false).unwrap();
    assert_eq!(operation.snapshots.len(), 3);
    assert_eq!(operation.changes.len(), 3);
    let roots = [0, 2].map(|index| {
        ItemSaveV5::decode_or_migrate(&operation.snapshots[index].bytes, None).unwrap()
    });
    for root in &roots {
        assert_eq!(
            root.placement,
            bace_storage_codec::ItemPlacementV2::World(position.clone())
        );
        assert_eq!(
            root.entity
                .state
                .properties
                .positions
                .iter()
                .find(|p| p.id == 1)
                .unwrap()
                .value,
            position
        );
    }
    assert_eq!(
        roots[0]
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .find(|p| p.id == 6)
            .unwrap()
            .value,
        proposal.victim.0
    );
    assert!(
        roots[1]
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .all(|p| p.id != 6)
    );
    let nested = ItemSaveV5::decode_or_migrate(&operation.snapshots[1].bytes, None).unwrap();
    assert_eq!(
        nested.placement,
        bace_storage_codec::ItemPlacementV2::Contained {
            container: items[0].object_id,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        }
    );
    assert!(freeze(&proposal, &items, true).is_err());

    let mut early_return = proposal.clone();
    early_return.olthoi_killer = true;
    let native = early_return.native.as_mut().unwrap();
    native.generated.clear();
    native.source_items = Some(vec![]);
    native.source_parents = Some(vec![]);
    early_return.drops.clear();
    let empty = freeze(&early_return, &[], true).unwrap();
    assert!(empty.snapshots.is_empty());
    assert!(empty.changes.is_empty());
}
