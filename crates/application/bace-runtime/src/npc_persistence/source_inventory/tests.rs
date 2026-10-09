use super::*;
use bace_content::{Property, WeenieV1};
use bace_types::EntityId;
use std::sync::Arc;
fn fixture() -> (
    tempfile::TempDir,
    crate::npc_sources::PreparedNpcRegistration,
    bace_simulation::NpcSourceCheckpoint,
) {
    fixture_for(10, false)
}
fn fixture_for(
    kind: u32,
    static_shop: bool,
) -> (
    tempfile::TempDir,
    crate::npc_sources::PreparedNpcRegistration,
    bace_simulation::NpcSourceCheckpoint,
) {
    let directory = tempfile::tempdir().unwrap();
    let source = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "npc_root".into(),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    };
    let mut records = vec![bace_storage_codec::PackRecord {
        key: bace_storage_codec::PackKey {
            namespace: 1,
            id: 100,
        },
        schema: 1,
        value: Some(bace_content_tools::compile_template(&source).unwrap()),
    }];
    if static_shop {
        records.push(bace_storage_codec::PackRecord {
            key: bace_storage_codec::PackKey {
                namespace: 20,
                id: 2,
            },
            schema: 1,
            value: Some(
                bace_content_tools::compile_world_record(
                    &bace_content::WorldRecordV1::LandblockInstance(
                        bace_content::LandblockInstanceRowV1 {
                            guid: 2,
                            landblock: 1,
                            weenie_class_id: 100,
                            obj_cell_id: 0x10001,
                            origin_x: 1.,
                            origin_y: 2.,
                            origin_z: 0.5,
                            angles_w: 1.,
                            angles_x: 0.,
                            angles_y: 0.,
                            angles_z: 0.,
                            is_link_child: false,
                            last_modified: String::new(),
                        },
                    ),
                )
                .unwrap(),
            ),
        });
    }
    let base = bace_storage_codec::compile_pack(
        directory.path(),
        records.into_iter().map(Ok),
        Default::default(),
    )
    .unwrap();
    let manifest = bace_storage_codec::PackManifest {
        version: 1,
        generation: 1,
        base,
        deltas: vec![],
    };
    let hash = manifest.content_hash(Default::default()).unwrap();
    let generation = Arc::new(manifest.open(directory.path(), Default::default()).unwrap());
    let registration =
        crate::npc_sources::prepare_registration(EntityId(2), 1, 1, hash, generation, 100).unwrap();
    let checkpoint = bace_simulation::NpcSourceCheckpoint {
        inventory: Some(NpcSourceInventorySnapshot {
            source: EntityId(2),
            ticket: 91,
            origin: None,
            items: vec![],
            root_item: None,
            source_registry_revision: Some(4),
            source_enchantments: vec![],
            death_items: vec![],
        }),
        location: Some(bace_simulation::NpcSourceLocation {
            cell: bace_types::CellId(0x10001),
            position: bace_geometry::Vec3::new(1., 2., 0.5),
            heading: 0.,
            facts: bace_emotes::NpcActorFacts {
                player: false,
                creature: true,
            },
        }),
        properties: Some(registration.source.properties.clone()),
        source_quests: Some((0, vec![])),
        archive: None,
        source: EntityId(2),
        active_operation: 1,
        invocations: vec![bace_simulation::NpcInvocationCheckpoint {
            operation: 1,
            event_id: [7; 16],
            key_version: 1,
            random_position: 0,
        }],
        logical_now: 1.,
        event_id: [7; 16],
        key_version: 1,
        random_position: 0,
        vm: bace_emotes::NativeCheckpoint {
            order: 0,
            remaining: 100,
            work: vec![],
            pending: vec![],
            detached: vec![],
        },
        pending: vec![],
    };
    (directory, registration, checkpoint)
}
fn operation(
    registration: &crate::npc_sources::PreparedNpcRegistration,
    checkpoint: bace_simulation::NpcSourceCheckpoint,
    version: u64,
) -> NpcStageOperation {
    let binding = crate::npc_persistence::NpcCheckpointBinding {
        source_version: version,
        source_template: 100,
        source: 2,
        invocation: checkpoint.event_id,
        program_hash: registration.source.program_hash,
        content_generation: registration.content_hash,
    };
    crate::npc_persistence::freeze_terminal_checkpoint(binding, 0, 1, checkpoint)
        .unwrap()
        .operation()
        .clone()
}
#[test]
fn source_aggregate_advances_on_journal_only_stage_and_exact_join_is_retryable() {
    let (_dir, mut registration, mut checkpoint) = fixture();
    let frozen = freeze_source_inventory(
        checkpoint.inventory.as_ref().unwrap(),
        &[],
        1,
        &registration,
    )
    .unwrap();
    let mut first = operation(&registration, checkpoint.clone(), 0);
    assert!(!joined(&first));
    frozen.attach(&mut first).unwrap();
    assert!(joined(&first));
    let bytes = first.workflow.checkpoint.clone();
    let rows = first.inventory.snapshots.clone();
    frozen.attach(&mut first).unwrap();
    assert_eq!(first.workflow.checkpoint, bytes);
    assert_eq!(first.inventory.snapshots, rows);
    let root = frozen.root_snapshot().unwrap();
    assert_eq!(root.mutation_revision, 1);
    registration.baseline = Some(bace_persistence::StoredAggregate {
        object_id: 2,
        persisted_version: 1,
        bytes: root.bytes.clone(),
    });
    checkpoint.event_id = [8; 16];
    checkpoint.invocations[0].event_id = [8; 16];
    checkpoint.inventory.as_mut().unwrap().ticket = 92;
    let frozen = freeze_source_inventory(
        checkpoint.inventory.as_ref().unwrap(),
        &[],
        1,
        &registration,
    )
    .unwrap();
    let mut next = operation(&registration, checkpoint, 1);
    frozen.attach(&mut next).unwrap();
    let root = frozen.root_snapshot().unwrap();
    assert_eq!((root.expected_version, root.mutation_revision), (1, 2));
    assert_eq!(
        NpcWorkflowSaveV3::decode(&next.workflow.checkpoint)
            .unwrap()
            .live_properties
            .unwrap()
            .revision,
        0
    );
}
#[test]
fn held_gear_and_source_root_are_one_idempotent_joint_operation() {
    let (_dir, registration, mut checkpoint) = fixture();
    let item = bace_inventory::InventoryItem {
        structure: None,
        id: EntityId(10),
        revision: 1,
        template: 101,
        stack_key: 1,
        place: bace_inventory::ItemPlace::Contained {
            container: EntityId(2),
            slot: 0,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
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
    checkpoint
        .inventory
        .as_mut()
        .unwrap()
        .items
        .push(bace_simulation::RegionUnloadItem {
            item,
            transient: true,
            container: None,
            registry_revision: Some(0),
            enchantments: vec![],
            position: None,
            corpse: None,
        });
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 101,
        class_name: "gear".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.ints = vec![Property { id: 12, value: 1 }];
    let source = FrozenInventoryItem {
        source_destination: None,
        corpse: None,
        construction: None,
        enchantments: vec![],
        entity: bace_storage_codec::EntitySaveV1 {
            object_id: 10,
            template_revision: 1,
            mutation_revision: 1,
            state,
        },
        placement: Some(bace_storage_codec::ItemPlacementV2::Contained {
            container: 2,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        }),
        persisted_version: 0,
    };
    let frozen = freeze_source_inventory(
        checkpoint.inventory.as_ref().unwrap(),
        &[source],
        1,
        &registration,
    )
    .unwrap();
    let mut operation = operation(&registration, checkpoint, 0);
    frozen.attach(&mut operation).unwrap();
    frozen.attach(&mut operation).unwrap();
    assert!(joined(&operation));
    assert_eq!(operation.inventory.snapshots.len(), 2);
    assert_eq!(operation.inventory.changes.len(), 1);
    let proof = NpcWorkflowSaveV3::decode(&operation.workflow.checkpoint)
        .unwrap()
        .inventory
        .unwrap();
    assert_eq!(proof.items[0].persisted_version, 1);
    assert_eq!(proof.source_persisted_version, 1);
}

#[test]
fn held_static_shop_checkpoint_freezes_world_v5_and_exact_replay() {
    let (_dir, mut registration, mut checkpoint) = fixture_for(12, true);
    assert!(is_authored_static_shop(&registration).unwrap());
    let frozen = freeze_source_inventory(
        checkpoint.inventory.as_ref().unwrap(),
        &[],
        1,
        &registration,
    )
    .unwrap();
    assert!(frozen.root_is_static_shop());
    let mut first = operation(&registration, checkpoint.clone(), 0);
    frozen.attach(&mut first).unwrap();
    assert!(joined(&first));
    assert_eq!(first.inventory.snapshots.len(), 1);
    assert_eq!(first.inventory.changes.len(), 1);
    assert_eq!(first.inventory.changes[0].expected, None);
    assert_eq!(
        first.inventory.changes[0].destination,
        bace_persistence::DurableItemPlace::World { cell: 0x10001 }
    );
    let root = frozen.root_snapshot().unwrap();
    let saved = bace_storage_codec::ItemSaveV5::decode(&root.bytes).unwrap();
    assert_eq!(saved.entity.object_id, 2);
    assert_eq!(saved.entity.state.weenie_type, 12);
    assert!(saved.construction.is_none());
    assert_eq!(
        saved.placement,
        bace_storage_codec::ItemPlacementV2::World(bace_content::Position {
            obj_cell_id: 0x10001,
            position_x: 1.,
            position_y: 2.,
            position_z: 0.5,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        })
    );
    let bytes = first.workflow.checkpoint.clone();
    frozen.attach(&mut first).unwrap();
    assert_eq!(first.workflow.checkpoint, bytes);
    assert_eq!(first.inventory.snapshots[0], root.clone());

    registration.baseline = Some(bace_persistence::StoredAggregate {
        object_id: 2,
        persisted_version: 1,
        bytes: root.bytes.clone(),
    });
    checkpoint.event_id = [8; 16];
    checkpoint.invocations[0].event_id = [8; 16];
    checkpoint.inventory.as_mut().unwrap().ticket = 92;
    let later = freeze_source_inventory(
        checkpoint.inventory.as_ref().unwrap(),
        &[],
        1,
        &registration,
    )
    .unwrap();
    let mut next = operation(&registration, checkpoint, 1);
    later.attach(&mut next).unwrap();
    assert_eq!(later.root_snapshot().unwrap().expected_version, 1);
    assert_eq!(later.root_snapshot().unwrap().mutation_revision, 2);
    assert_eq!(
        next.inventory.changes[0].expected,
        Some(bace_persistence::DurableItemPlace::World { cell: 0x10001 })
    );
}

#[test]
fn generated_or_unindexed_shop_cannot_become_a_static_world_source() {
    let (_dir, registration, checkpoint) = fixture_for(12, false);
    assert!(!is_authored_static_shop(&registration).unwrap());
    let frozen = freeze_source_inventory(
        checkpoint.inventory.as_ref().unwrap(),
        &[],
        1,
        &registration,
    )
    .unwrap();
    assert!(!frozen.root_is_static_shop());
    let mut operation = operation(&registration, checkpoint, 0);
    frozen.attach(&mut operation).unwrap();
    assert!(operation.inventory.changes.is_empty());
    assert!(
        bace_storage_codec::ItemSaveV5::decode(&frozen.root_snapshot().unwrap().bytes).is_err()
    );
}
