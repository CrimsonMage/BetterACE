use bace_gameplay_api::{NpcCompletion, NpcContext};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_runtime::npc_persistence::{
    NpcCheckpointBinding, NpcExperienceAdmissionInput, freeze_experience_admission,
};
use bace_simulation::{
    NpcEffect, NpcInvocationCheckpoint, NpcPendingCheckpoint, NpcProposal,
    NpcQueuedExperiencePhase as Phase, NpcSourceCheckpoint,
};
use bace_storage_codec::{NpcSharingPolicyV2, NpcWorkflowSaveV2};
use bace_types::EntityId;
fn input() -> NpcExperienceAdmissionInput {
    let context = NpcContext {
        source: EntityId(0x80000001),
        target: Some(EntityId(0x50000001)),
        operation: 1,
    };
    let proposal = NpcProposal {
        ticket: 7,
        context,
        effect: NpcEffect::QueuedExperience {
            share: bace_simulation::NpcExperienceSharing::None,
            actor: EntityId(0x50000001),
            amount: 100,
            phase: Phase::AwaitingAdmission,
        },
    };
    let mut ready = proposal.clone();
    if let NpcEffect::QueuedExperience { phase, .. } = &mut ready.effect {
        *phase = Phase::Ready;
    }
    NpcExperienceAdmissionInput {
        binding: NpcCheckpointBinding {
            source_version: 0,
            source_template: 100,
            invocation: [1; 16],
            source: context.source.0,
            program_hash: [2; 32],
            content_generation: [3; 32],
        },
        stage: 0,
        world_epoch: 5,
        workflow_version: 0,
        proposal,
        committed_checkpoint: NpcSourceCheckpoint {
            inventory: None,
            location: None,
            properties: None,
            source_quests: None,
            archive: None,
            source: context.source,
            active_operation: 1,
            invocations: vec![NpcInvocationCheckpoint {
                operation: 1,
                event_id: [4; 16],
                key_version: 1,
                random_position: 8,
            }],
            logical_now: 3.0,
            event_id: [4; 16],
            key_version: 1,
            random_position: 8,
            vm: bace_emotes::NativeCheckpoint {
                order: 1,
                remaining: 100,
                work: vec![],
                pending: vec![],
                detached: vec![7],
            },
            pending: vec![NpcPendingCheckpoint {
                proposal: ready,
                completion: NpcCompletion::Applied { post_delay: 0.0 },
                adopted: false,
                detached: true,
            }],
        },
        lease: CharacterLease {
            character_id: 0x50000001,
            epoch: 3,
            state: OwnershipState::Online,
        },
    }
}
#[test]
fn queue_admission_persists_detached_continuation_without_mutating_a_player_snapshot() {
    let pending = freeze_experience_admission(input()).unwrap();
    let operation = pending.operation();
    assert!(operation.inventory.snapshots.is_empty());
    assert!(operation.inventory.changes.is_empty());
    assert_eq!(
        operation.inventory.participants,
        vec![0x50000001, 0x80000001]
    );
    assert_eq!(operation.inventory.leases[0].epoch, 3);
    assert_eq!(operation.workflow.world_epoch, 5);
    let checkpoint =
        bace_storage_codec::NpcWorkflowSaveV3::decode(&operation.workflow.checkpoint).unwrap();
    assert_eq!(checkpoint.detached, vec![7]);
    assert_eq!(checkpoint.effects.len(), 1);
    assert!(!checkpoint.effects[0].adopted);
    assert!(checkpoint.effects[0].detached);
    assert_eq!(checkpoint.random_position, 8);
    assert_eq!(checkpoint.logical_now, 3.0);
    assert!(!checkpoint.completed);
}
#[test]
fn allegiance_only_policy_survives_binary_restart_and_v1_defaults_are_explicit() {
    let mut input = input();
    for proposal in [
        &mut input.proposal,
        &mut input.committed_checkpoint.pending[0].proposal,
    ] {
        if let NpcEffect::QueuedExperience { share, .. } = &mut proposal.effect {
            *share = bace_simulation::NpcExperienceSharing::Allegiance;
        }
    }
    let binding = input.binding;
    let pending = freeze_experience_admission(input).unwrap();
    let frozen =
        bace_storage_codec::NpcWorkflowSaveV3::decode(&pending.operation().workflow.checkpoint)
            .unwrap()
            .previous;
    assert_eq!(
        frozen.experience_sharing[0].policy,
        NpcSharingPolicyV2::Allegiance
    );
    let restored =
        bace_runtime::npc_persistence::restore_checkpoint(binding, frozen.clone()).unwrap();
    assert!(matches!(
        restored.pending[0].proposal.effect,
        NpcEffect::QueuedExperience {
            share: bace_simulation::NpcExperienceSharing::Allegiance,
            ..
        }
    ));
    let legacy = frozen.previous.encode().unwrap();
    let migrated = NpcWorkflowSaveV2::decode_or_migrate(&legacy).unwrap();
    assert_eq!(
        migrated.experience_sharing[0].policy,
        NpcSharingPolicyV2::None
    );
    let mut missing = frozen.clone();
    missing.experience_sharing.clear();
    assert!(missing.encode().is_err());
    let mut duplicate = frozen.clone();
    duplicate
        .experience_sharing
        .push(duplicate.experience_sharing[0]);
    assert!(duplicate.encode().is_err());
    let mut mismatched = frozen;
    mismatched.experience_sharing[0].policy = NpcSharingPolicyV2::All;
    assert!(mismatched.encode().is_err());
}
#[test]
fn already_applied_wrong_participant_and_non_detached_previews_are_rejected() {
    let mut bad = input();
    bad.committed_checkpoint.pending[0].adopted = true;
    assert!(freeze_experience_admission(bad).is_err());
    let mut bad = input();
    bad.committed_checkpoint.pending[0].detached = false;
    assert!(freeze_experience_admission(bad).is_err());
    let mut bad = input();
    bad.lease.character_id += 1;
    assert!(freeze_experience_admission(bad).is_err());
    let mut bad = input();
    if let NpcEffect::QueuedExperience { amount, .. } =
        &mut bad.committed_checkpoint.pending[0].proposal.effect
    {
        *amount = 101;
    }
    assert!(freeze_experience_admission(bad).is_err());
}

#[test]
fn detached_service_stage_persists_only_continuation_admission() {
    use bace_runtime::npc_persistence::{
        NpcServiceAdmissionInput, NpcStageAdoption, freeze_service_admission,
    };
    let mut source = input();
    source.proposal.effect = NpcEffect::Service(bace_gameplay_api::NpcOperation::Give {
        template: 100,
        count: 1,
        palette: 0,
        shade: 0.0,
    });
    source.committed_checkpoint.pending[0].proposal = source.proposal.clone();
    source.committed_checkpoint.pending[0].completion = NpcCompletion::Applied { post_delay: 1.5 };
    let pending = freeze_service_admission(NpcServiceAdmissionInput {
        binding: source.binding,
        stage: 0,
        world_epoch: 5,
        workflow_version: 0,
        proposal: source.proposal,
        post_delay: 1.5,
        committed_checkpoint: source.committed_checkpoint,
        leases: vec![source.lease],
    })
    .unwrap();
    assert!(pending.operation().inventory.snapshots.is_empty());
    assert!(pending.operation().inventory.changes.is_empty());
    let checkpoint =
        bace_storage_codec::NpcWorkflowSaveV3::decode(&pending.operation().workflow.checkpoint)
            .unwrap();
    assert!(checkpoint.effects[0].detached);
    assert!(!checkpoint.effects[0].adopted);
    let _type_contract = NpcStageAdoption::DetachedService { post_delay: 1.5 };
}

#[test]
fn detached_source_schema3_preserves_properties_and_migrates_old_workflows() {
    let mut input = input();
    let source = input.binding.source;
    let binding = input.binding;
    input.committed_checkpoint.archive = Some(bace_simulation::NpcSourceArchive {
        source: EntityId(source),
        facts: bace_emotes::NpcActorFacts {
            player: false,
            creature: true,
        },
        cell: bace_types::CellId(1),
        position: bace_geometry::Vec3::new(4.0, 5.0, 0.5),
        heading: 0.25,
        properties: bace_entity::EntityProperties::restore_snapshot(
            91,
            vec![(
                bace_entity::PropertyFamily::String,
                1,
                bace_entity::PropertyValue::String("Keeper".into()),
            )],
        )
        .unwrap(),
    });
    let pending = freeze_experience_admission(input).unwrap();
    let decoded =
        bace_storage_codec::NpcWorkflowSaveV3::decode(&pending.operation().workflow.checkpoint)
            .unwrap();
    assert_eq!(decoded.archive.as_ref().unwrap().property_revision, 91);
    let restored =
        bace_runtime::npc_persistence::restore_checkpoint(binding, decoded.clone()).unwrap();
    assert_eq!(
        restored
            .archive
            .unwrap()
            .properties
            .get(bace_entity::PropertyFamily::String, 1),
        Some(&bace_entity::PropertyValue::String("Keeper".into()))
    );
    let old = decoded.previous.encode().unwrap();
    assert!(
        bace_storage_codec::NpcWorkflowSaveV3::decode_or_migrate(&old)
            .unwrap()
            .archive
            .is_none()
    );
    let mut wrong = decoded.clone();
    wrong.archive.as_mut().unwrap().source += 1;
    assert!(wrong.encode().is_err());
    let mut duplicate = decoded;
    let value = duplicate.archive.as_ref().unwrap().properties[0].clone();
    duplicate.archive.as_mut().unwrap().properties.push(value);
    assert!(duplicate.encode().is_err());
}

#[tokio::test]
async fn coordinator_retains_exact_uncertain_write_and_failed_owner_adoption() {
    use bace_persistence::{NpcStageOperation, OperationOutcome, SaveAck, SaveSnapshot};
    use bace_runtime::{
        npc_service::{NpcCoordinator, NpcCoordinatorEvent, NpcDurableAdoption},
        saves::{SaveBackend, SaveFailure, SaveWorkerConfig, spawn_save_worker},
        simulation::{SimulationConfig, SimulationWorker},
    };
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    struct Backend(Arc<Mutex<Vec<Vec<u8>>>>);
    impl SaveBackend for Backend {
        async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
            Ok(vec![])
        }
        async fn valuable(
            &self,
            _: &str,
            _: &[SaveSnapshot],
        ) -> Result<OperationOutcome, SaveFailure> {
            Ok(OperationOutcome::AlreadyCommitted)
        }
        async fn npc_stage(
            &self,
            operation: &NpcStageOperation,
        ) -> Result<OperationOutcome, SaveFailure> {
            let mut calls = self.0.lock().unwrap();
            calls.push(operation.workflow.checkpoint.clone());
            if calls.len() == 1 {
                Err(SaveFailure::Storage {
                    message: "commit response lost".into(),
                    uncertain: true,
                })
            } else {
                Ok(OperationOutcome::AlreadyCommitted)
            }
        }
    }
    let fixture = input();
    let binding = fixture.binding;
    let source = EntityId(binding.source);
    let pending = freeze_experience_admission(fixture).unwrap();
    let exact = pending.operation().workflow.checkpoint.clone();
    let backend = Backend(Arc::new(Mutex::new(vec![])));
    let saves = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let simulation = SimulationWorker::spawn(
        bace_simulation::Kernel::with_gameplay_limits(bace_world::World::default(), 8, 8, 1)
            .unwrap(),
        SimulationConfig {
            command_capacity: 8,
            tick_limit: None,
            real_time: true,
        },
    )
    .unwrap();
    let mut coordinator = NpcCoordinator::new(4).unwrap();
    coordinator.bind(binding, 0).unwrap();
    assert!(
        coordinator
            .enqueue_stage(source, pending, NpcDurableAdoption::Player)
            .is_ok()
    );
    let mut uncertain = false;
    let mut committed = false;
    let mut adoption_failed = false;
    for _ in 0..200 {
        coordinator.pump(&simulation.input(), &saves.handle);
        while let Ok(outcome) = simulation.npc_service_outcomes().try_recv() {
            assert!(coordinator.accept(outcome).is_ok());
        }
        while let Some(event) = coordinator.take_event() {
            match event {
                NpcCoordinatorEvent::Durable { resolution, .. }
                    if matches!(
                        resolution.as_ref(),
                        bace_runtime::npc_persistence::NpcStageResolution::Uncertain { .. }
                    ) =>
                {
                    uncertain = true;
                    assert_eq!(coordinator.binding(source).unwrap().0.source_version, 0);
                }
                NpcCoordinatorEvent::Durable { resolution, .. }
                    if matches!(
                        resolution.as_ref(),
                        bace_runtime::npc_persistence::NpcStageResolution::Committed { .. }
                    ) =>
                {
                    committed = true
                }
                NpcCoordinatorEvent::Command { outcome, .. } => {
                    assert!(outcome.result.is_err());
                    adoption_failed = true;
                }
                _ => {}
            }
        }
        if adoption_failed {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert!(
        uncertain && committed && adoption_failed,
        "uncertain={uncertain} committed={committed} adoption_failed={adoption_failed} calls={} binding={:?}",
        backend.0.lock().unwrap().len(),
        coordinator.binding(source)
    );
    assert!(coordinator.busy(source));
    assert_eq!(coordinator.binding(source).unwrap().0.source_version, 1);
    assert_eq!(coordinator.binding(source).unwrap().1, 1);
    assert!(
        coordinator
            .enqueue_stage(
                source,
                freeze_experience_admission(input()).unwrap(),
                NpcDurableAdoption::Player
            )
            .is_err()
    );
    assert_eq!(*backend.0.lock().unwrap(), vec![exact.clone(), exact]);
    let exit = simulation.shutdown_recover().unwrap();
    assert!(exit.failure.is_none());
    saves.handle.close();
    assert_eq!(saves.task.await.unwrap().valuable_completed, 1);
}
