use super::*;
use crate::saves::{SaveBackend, SaveFailure, SaveWorkerConfig, spawn_save_worker};
use bace_persistence::{OperationOutcome, PlacementOperation, SaveAck};
use std::{sync::Mutex, time::Duration};
#[derive(Clone, Default)]
struct Backend {
    seen: Arc<Mutex<Vec<PlacementOperation>>>,
}
impl SaveBackend for Backend {
    async fn placement(&self, op: &PlacementOperation) -> Result<OperationOutcome, SaveFailure> {
        let mut seen = self.seen.lock().unwrap();
        seen.push(op.clone());
        if seen.len() == 1 {
            return Err(SaveFailure::Timeout);
        }
        Ok(OperationOutcome::Committed(
            op.snapshots
                .iter()
                .map(|s| SaveAck {
                    object_id: s.object_id,
                    mutation_revision: s.mutation_revision,
                    persisted_version: s.expected_version + 1,
                })
                .collect(),
        ))
    }
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("wrong save lane")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("unfenced portal save")
    }
}
fn fixture() -> (bace_simulation::Kernel, OnlinePlayerSaveService, PortalWork) {
    let (mut kernel, online, craft) = crate::crafting_service::tests::fixture::fixture();
    kernel.reject_crafting(craft.ticket.operation).unwrap();
    let binding = craft.binding;
    let destination = bace_interactions::PortalPosition {
        cell: 1,
        origin: [4.0, 4.0, 0.5],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    kernel
        .register_portal_links(
            binding.actor,
            bace_interactions::PortalLinks::new(0, &[], &[]).unwrap(),
            bace_interactions::PortalAccess {
                level: 275,
                pk_status: 2,
                pk_recent: false,
                olthoi: false,
                vitae: false,
                account_15_days: true,
                entitlement: u32::MAX,
                quest_allowed: true,
                teleporting: false,
                recently_teleported: false,
                ignore_restrictions: false,
                enforce_maximum_level: true,
            },
        )
        .unwrap();
    kernel
        .register_recall_locations(Arc::new(bace_simulation::PreparedRecallLocations {
            marketplace: destination,
            pk_arena: [destination; 5],
            pkl_arena: [destination; 5],
        }))
        .unwrap();
    kernel
        .apply_recall_command(bace_simulation::RecallCommand::Start {
            context: bace_gameplay_api::ActionContext {
                actor: binding.actor,
                account: binding.account,
                session: binding.session,
                sequence: 4,
            },
            kind: bace_interactions::RecallKind::Marketplace,
            animation_seconds: 1.0,
        })
        .unwrap();
    let ticket = (0..450)
        .find_map(|_| {
            kernel.step().unwrap();
            kernel.take_portal_proposal()
        })
        .expect("source timed recall proposal");
    (
        kernel,
        online,
        PortalWork {
            epoch: 7,
            bindings: vec![binding],
            ticket,
        },
    )
}

#[test]
fn portal_changed_item_snapshot_keeps_durable_source_origin() {
    let (_, online, work) = fixture();
    let mut item = online.inventory_baselines(work.ticket.actor.0).remove(0);
    item.source_destination = Some(2);
    item.entity.mutation_revision += 1;
    let expected_id = item.entity.object_id;
    let expected_version = item.persisted_version;
    let saved = super::freezing::freeze_portal_item(item).unwrap();
    let restored = bace_storage_codec::ItemSaveV5::decode(&saved.encode().unwrap()).unwrap();
    assert_eq!(restored.entity.object_id, expected_id);
    assert_eq!(restored.entity.mutation_revision, 2);
    assert_eq!(restored.source_destination, Some(2));
    assert_eq!(expected_version, 4);
}
#[tokio::test]
async fn uncertain_portal_reuses_exact_save_and_waits_for_effect_before_baseline_release() {
    uncertain_portal(false).await;
}
#[tokio::test]
async fn effect_before_receipt_ack_is_retained_without_repeating_teleport() {
    uncertain_portal(true).await;
}
async fn uncertain_portal(early_effect: bool) {
    let (mut kernel, mut online, work) = fixture();
    let actor = work.ticket.actor;
    let operation = work.ticket.operation;
    let version = online.baseline(actor.0).unwrap().1;
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut service = PortalService::new();
    assert!(service.stage(work).is_ok());
    let mut token = 100;
    let mut pressure = 2;
    let mut effect = None;
    let mut deferred_ack = None;
    tokio::time::timeout(Duration::from_secs(10), async {
        while service.completion.is_none() {
            token += 1;
            if let Err(error) = service.poll_with(token, &mut online, &worker.handle, |command| {
                if matches!(command, Command::PortalResolution(_)) && pressure > 0 {
                    pressure -= 1;
                    return Err(Box::new(TrySendError::Full(command)));
                }
                kernel
                    .try_enqueue(command)
                    .map_err(|c| Box::new(TrySendError::Full(c)))
            }) {
                assert!(
                    matches!(
                        service.pending.as_ref().unwrap().phase,
                        Phase::Saving { .. }
                    ),
                    "{error}"
                );
                assert!(service.reject_unsubmitted(token + 1).is_err());
                service.retry();
            }
            if service.completion.is_some() {
                break;
            }
            assert_eq!(online.baseline(actor.0).unwrap().1, version);
            kernel.step().unwrap();
            while let Some(capture) = kernel.take_player_snapshot_outcome() {
                service.accept_capture(capture, 25_000).unwrap();
            }
            while let Some(outcome) = kernel.take_portal_resolution() {
                let mut unrelated = outcome.clone();
                unrelated.correlation += 1;
                assert!(service.accept_resolution(unrelated).is_err());
                assert_eq!(online.baseline(actor.0).unwrap().1, version);
                if early_effect {
                    assert!(deferred_ack.replace(outcome).is_none());
                } else {
                    service.accept_resolution(outcome).unwrap();
                }
            }
            while let Some(event) = kernel.take_portal_event() {
                if matches!(event, PortalServiceEvent::Teleported { .. }) {
                    effect = Some(event)
                }
            }
            if let Some(event) = effect.take() {
                assert!(matches!(
                    service.pending.as_ref().unwrap().phase,
                    Phase::Effect { .. } | Phase::Delivering { .. }
                ));
                assert!(
                    !service.accept_effect(&PortalServiceEvent::Linked { operation, actor }),
                    "wrong effect cannot adopt teleport baseline"
                );
                assert!(!service.accept_effect(&PortalServiceEvent::Teleported {
                    operation: operation + 1,
                    actors: vec![actor],
                    views: vec![],
                }));
                assert!(service.accept_effect(&event));
                if let Some(outcome) = deferred_ack.take() {
                    assert_eq!(online.baseline(actor.0).unwrap().1, version);
                    service.accept_resolution(outcome).unwrap();
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let completion = service.take_completion().unwrap();
    assert!(completion.committed && !completion.aborted);
    assert_eq!(pressure, 0);
    assert_eq!(online.baseline(actor.0).unwrap().1, version + 1);
    {
        let rows = backend.seen.lock().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].operation_id, rows[1].operation_id);
        assert_eq!(rows[0].snapshots, rows[1].snapshots);
        assert_eq!(rows[0].participants, rows[1].participants);
        assert_eq!(rows[0].leases, rows[1].leases);
        assert_eq!(rows[0].changes, rows[1].changes);
        assert_eq!(rows[0].storage_views, rows[1].storage_views);
        let saved = bace_storage_codec::PlayerSaveV6::decode(
            &rows[1]
                .snapshots
                .iter()
                .find(|s| s.object_id == actor.0)
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert_eq!(crate::ui_saves::restore_ui(&saved).unwrap().filters, 3);
        assert_eq!(
            saved
                .player
                .entity
                .state
                .properties
                .positions
                .iter()
                .find(|p| p.id == 1)
                .unwrap()
                .value
                .position_x,
            4.0
        );
    }
    worker.handle.close();
    worker.task.await.unwrap();
}
#[tokio::test]
async fn cancellation_before_capture_releases_exact_owner_without_a_write() {
    let (mut kernel, mut online, work) = fixture();
    let operation = work.ticket.operation;
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut service = PortalService::new();
    assert!(service.stage(work).is_ok());
    service.reject_unsubmitted(100).unwrap();
    for token in 101..105 {
        service
            .poll_with(token, &mut online, &worker.handle, |c| {
                kernel
                    .try_enqueue(c)
                    .map_err(|c| Box::new(TrySendError::Full(c)))
            })
            .unwrap();
        kernel.step().unwrap();
        while let Some(outcome) = kernel.take_portal_resolution() {
            service.accept_resolution(outcome).unwrap();
        }
    }
    assert!(!service.take_completion().unwrap().committed);
    assert!(kernel.pending_portal_proposal(operation).is_none());
    assert!(backend.seen.lock().unwrap().is_empty());
    worker.handle.close();
    worker.task.await.unwrap();
}

#[test]
fn duplicate_participants_cannot_acquire_a_partial_critical_barrier() {
    let (_, _, mut work) = fixture();
    work.bindings.push(work.bindings[0]);
    work.ticket.participants.push(work.ticket.participants[0]);
    let mut service = PortalService::new();
    assert!(service.stage(work).is_err());
    assert!(!service.requires_drain());
}
