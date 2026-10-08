use super::*;
use bace_content::{Property, WeenieV1};
use bace_types::EntityId;
use std::sync::Arc;
fn fixture() -> (
    tempfile::TempDir,
    crate::npc_sources::PreparedNpcRegistration,
    bace_simulation::NpcSourceCheckpoint,
) {
    let directory = tempfile::tempdir().unwrap();
    let source = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "npc_root".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    let base = bace_storage_codec::compile_pack(
        directory.path(),
        [Ok(bace_storage_codec::PackRecord {
            key: bace_storage_codec::PackKey {
                namespace: 1,
                id: 100,
            },
            schema: 1,
            value: Some(bace_content_tools::compile_template(&source).unwrap()),
        })],
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
