use bace_content::{Emote, EmoteAction, Property, WeenieV1};
use bace_db_postgres::StoredNpcSourceHead;
use bace_runtime::npc_recovery::{NpcRecoveryRequest, NpcRecoveryWorker, prepare_npc_source};
use bace_storage_codec::{
    NpcSourceArchiveV3, NpcWorkflowSaveV1, NpcWorkflowSaveV3, PackKey, PackLimits, PackManifest,
    PackRecord,
    npc_values_v1::NpcContextV1,
    npc_workflow_v1::{NpcInvocationSaveV1, NpcScheduledRowV1},
};
fn fixture() -> (tempfile::TempDir, NpcRecoveryRequest) {
    fixture_with_radius(None)
}
fn fixture_with_radius(radius: Option<f64>) -> (tempfile::TempDir, NpcRecoveryRequest) {
    let directory = tempfile::tempdir().unwrap();
    let mut value = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "keeper".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    value.properties.strings.push(Property {
        id: 1,
        value: "Keeper".into(),
    });
    if let Some(value_radius) = radius {
        value.properties.floats.push(Property {
            id: 54,
            value: value_radius,
        });
    }
    value.properties.emotes.push(Emote {
        category: 7,
        probability: 1.,
        actions: vec![EmoteAction {
            r#type: 8,
            message: Some("still here".into()),
            ..Default::default()
        }],
        ..Default::default()
    });
    let bytes = bace_content_tools::compile_template(&value).unwrap();
    let base = bace_storage_codec::compile_pack(
        directory.path(),
        [Ok(PackRecord {
            key: PackKey {
                namespace: 1,
                id: 100,
            },
            schema: 1,
            value: Some(bytes),
        })],
        PackLimits::default(),
    )
    .unwrap();
    let manifest = PackManifest {
        version: 1,
        generation: 1,
        base,
        deltas: vec![],
    };
    let hash = manifest.content_hash(PackLimits::default()).unwrap();
    let generation = manifest
        .open(directory.path(), PackLimits::default())
        .unwrap();
    let prepared = prepare_npc_source(&generation, 100).unwrap();
    let frozen = NpcWorkflowSaveV1 {
        invocation: [1; 16],
        source: 33,
        program_hash: prepared.program_hash,
        content_generation: hash,
        logical_now: 1.,
        event_id: [2; 16],
        key_version: 1,
        random_position: 1,
        stage: 0,
        completed: false,
        next_order: 1,
        remaining_instructions: 100,
        active_operation: 1,
        invocations: vec![NpcInvocationSaveV1 {
            operation: 1,
            event_id: [2; 16],
            key_version: 1,
            random_position: 1,
        }],
        scheduled: vec![NpcScheduledRowV1 {
            inline: true,
            depth: 0,
            set: 0,
            action: 0,
            due: 1.,
            order: 1,
            context: NpcContextV1 {
                source: 33,
                target: Some(44),
                operation: 1,
            },
        }],
        pending: vec![],
        detached: vec![],
        effects: vec![],
    };
    let mut frozen = NpcWorkflowSaveV3::from(frozen);
    frozen.source_version = 2;
    frozen.source_template = 100;
    frozen.archive = Some(NpcSourceArchiveV3 {
        source: 33,
        player: false,
        creature: true,
        cell: 1,
        position: [1., 2., 3.],
        heading: 0.,
        property_revision: 4,
        properties: vec![],
    });
    let request = NpcRecoveryRequest {
        head: StoredNpcSourceHead {
            source: 33,
            source_version: 2,
            source_template: 100,
            invocation: [1; 16],
            workflow_version: 1,
            completed: false,
            checkpoint: frozen.encode().unwrap(),
        },
        manifest,
        manifest_hash: hash,
    };
    (directory, request)
}
#[test]
fn authored_use_radius_preserves_signed_zero_and_large_finite_thresholds() {
    // ACE 47edade3 WorldObject_Use.cs IsWithinUseRadiusOf: only a missing
    // Float54 receives0.6; a signed value is an ordinary cylinder threshold.
    for radius in [None, Some(-0.1), Some(0.0), Some(500.)] {
        let (directory, request) = fixture_with_radius(radius);
        let generation = std::sync::Arc::new(
            request
                .manifest
                .open(directory.path(), Default::default())
                .unwrap(),
        );
        let registration = bace_runtime::npc_sources::prepare_registration(
            bace_types::EntityId(33),
            1,
            1,
            request.manifest_hash,
            generation,
            100,
        )
        .unwrap();
        assert_eq!(registration.use_radius, radius.unwrap_or(0.6) as f32);
        assert!(registration.script_source().valid_bounds());
    }
}
#[test]
fn archived_source_reopens_exact_template_without_a_world_body() {
    let (directory, request) = fixture();
    let worker = NpcRecoveryWorker::start(directory.path().into(), 1).unwrap();
    assert!(worker.try_submit(request).is_ok());
    assert!(worker.has_pending());
    let worker = match worker.try_shutdown() {
        Err(worker) => worker,
        Ok(_) => panic!("unobserved NPC recovery must remain retained"),
    };
    let mut outcomes = worker.shutdown().unwrap();
    let ready = outcomes.pop().unwrap().result.unwrap();
    assert!(outcomes.is_empty());
    assert!(ready.archived());
    assert_eq!(ready.binding.source_template, 100);
    assert_eq!(ready.binding.source_version, 2);
    assert_eq!(
        ready
            .participants
            .into_iter()
            .map(|id| id.0)
            .collect::<Vec<_>>(),
        vec![44]
    );
    assert_eq!(ready.checkpoint.archive.unwrap().properties.revision(), 4);
}
#[test]
fn wrong_pinned_program_or_source_locator_fails_recovery() {
    for wrong_template in [false, true] {
        let (directory, mut request) = fixture();
        let mut checkpoint = NpcWorkflowSaveV3::decode(&request.head.checkpoint).unwrap();
        if wrong_template {
            checkpoint.source_template = 101;
            request.head.source_template = 101;
        } else {
            checkpoint.previous.previous.program_hash = [99; 32];
        }
        request.head.checkpoint = checkpoint.encode().unwrap();
        let worker = NpcRecoveryWorker::start(directory.path().into(), 1).unwrap();
        assert!(worker.try_submit(request).is_ok());
        assert!(worker.shutdown().unwrap().pop().unwrap().result.is_err());
    }
}

#[test]
fn idle_recovery_worker_shutdown_transfers_join_handle_without_blocking() {
    let directory = tempfile::tempdir().unwrap();
    let worker = NpcRecoveryWorker::start(directory.path().into(), 1).unwrap();
    assert!(!worker.has_pending());
    match worker.try_shutdown() {
        Ok(handle) => handle.join().unwrap(),
        Err(_) => panic!("idle worker must be joinable"),
    }
}
