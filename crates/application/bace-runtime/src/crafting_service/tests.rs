use super::*;
pub(crate) mod fixture;
use crate::saves::{SaveBackend, SaveFailure, SaveWorkerConfig, spawn_save_worker};
use bace_persistence::{OperationOutcome, PlacementOperation, SaveAck};
use fixture::*;
use std::{collections::VecDeque, sync::Mutex, time::Duration};
#[derive(Clone, Default)]
struct Backend {
    seen: Arc<Mutex<Vec<PlacementOperation>>>,
    answers: Arc<Mutex<VecDeque<u8>>>,
}
impl SaveBackend for Backend {
    async fn placement(&self, op: &PlacementOperation) -> Result<OperationOutcome, SaveFailure> {
        self.seen.lock().unwrap().push(op.clone());
        match self.answers.lock().unwrap().pop_front().unwrap_or(0) {
            1 => Err(SaveFailure::Timeout),
            2 => Err(SaveFailure::Storage {
                message: "definite rejection after uncertain prior attempt".into(),
                uncertain: false,
            }),
            _ => Ok(OperationOutcome::Committed(
                op.snapshots
                    .iter()
                    .map(|s| SaveAck {
                        object_id: s.object_id,
                        mutation_revision: s.mutation_revision,
                        persisted_version: s.expected_version + 1,
                    })
                    .collect(),
            )),
        }
    }
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("wrong lane")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("unfenced write")
    }
}
#[tokio::test]
async fn live_tinker_preserves_unsaved_ui_and_retries_exact_bytes_before_owner_adoption() {
    let (mut kernel, mut online, work) = fixture();
    let actor = work.binding.actor;
    let binding = work.binding;
    assert!(
        kernel
            .read_player_operation_snapshot(
                binding,
                PlayerSnapshotOperation::Crafting(work.ticket.operation + 1),
                revision(&work.ticket)
            )
            .is_err()
    );
    let backend = Backend::default();
    backend.answers.lock().unwrap().extend([1, 2, 0]);
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut service = CraftingService::new();
    assert!(service.stage(work).is_ok());
    let mut pressure = 3;
    let mut uncertain = 0;
    let mut token = 100;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            token += 1;
            let result = service.poll_with(&mut online, &worker.handle, token, |command| {
                if matches!(command, Command::Crafting(_)) && pressure > 0 {
                    pressure -= 1;
                    return Err(Box::new(TrySendError::Full(command)));
                }
                kernel
                    .try_enqueue(command)
                    .map_err(|c| Box::new(TrySendError::Full(c)))
            });
            if let Err(error) = result {
                assert!(
                    matches!(
                        service.pending.as_ref().unwrap().phase,
                        Phase::Saving { .. }
                    ),
                    "{error}"
                );
                uncertain += 1;
                assert!(service.reject_unsubmitted(token + 1).is_err());
                service.retry();
            }
            if service.completion.is_some() {
                break;
            }
            assert_eq!(
                online.baseline(actor.0).unwrap().1,
                3,
                "baseline cannot advance before adoption"
            );
            kernel.step().unwrap();
            while let Some(capture) = kernel.take_player_snapshot_outcome() {
                assert!(capture.result.is_ok(), "{:?}", capture.result.err());
                service.accept_capture(capture, 10_000).unwrap();
            }
            while let Some(outcome) = kernel.take_crafting_outcome() {
                service.accept_outcome(outcome).unwrap();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(uncertain, 2);
    assert_eq!(pressure, 0);
    assert!(service.take_completion().unwrap().committed);
    assert!(!service.requires_drain());
    assert!(kernel.inventory_item(SOURCE).is_none());
    assert_eq!(kernel.inventory_item(TARGET).unwrap().revision, 2);
    assert_eq!(online.baseline(actor.0).unwrap().1, 4);
    {
        let seen = backend.seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        for pair in seen.windows(2) {
            assert_eq!(pair[0].operation_id, pair[1].operation_id);
            assert_eq!(pair[0].snapshots, pair[1].snapshots);
            assert_eq!(pair[0].participants, pair[1].participants);
            assert_eq!(pair[0].leases, pair[1].leases);
            assert_eq!(pair[0].changes, pair[1].changes);
        }
        let saved = bace_storage_codec::PlayerSaveV6::decode(
            &seen[0]
                .snapshots
                .iter()
                .find(|s| s.object_id == actor.0)
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert_eq!(saved.ui.spellbook_filters, 3);
        assert_eq!(saved.player.entity.mutation_revision, 4);
    }
    assert!(kernel.read_player_snapshot(binding).is_ok());
    worker.handle.close();
    worker.task.await.unwrap();
}
#[tokio::test]
async fn unsubmitted_cancellation_waits_for_exact_owner_ack_and_releases_baseline() {
    let (mut kernel, mut online, work) = fixture();
    let actor = work.binding.actor;
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut service = CraftingService::new();
    assert!(service.stage(work).is_ok());
    service
        .poll_with(&mut online, &worker.handle, 100, |_| panic!())
        .unwrap();
    service.reject_unsubmitted(101).unwrap();
    assert!(online.critical_ready(&[actor.0]).is_err());
    service
        .poll_with(&mut online, &worker.handle, 102, |command| {
            kernel
                .try_enqueue(command)
                .map_err(|c| Box::new(TrySendError::Full(c)))
        })
        .unwrap();
    kernel.step().unwrap();
    let mut outcome = kernel.take_crafting_outcome().unwrap();
    outcome.correlation += 1;
    outcome = *service.accept_outcome(outcome).unwrap_err();
    outcome.correlation -= 1;
    service.accept_outcome(outcome).unwrap();
    service
        .poll_with(&mut online, &worker.handle, 103, |_| panic!())
        .unwrap();
    assert!(!service.take_completion().unwrap().committed);
    assert!(online.critical_ready(&[actor.0]).unwrap());
    assert!(backend.seen.lock().unwrap().is_empty());
    assert!(kernel.inventory_item(SOURCE).is_some());
    worker.handle.close();
    worker.task.await.unwrap();
}

#[tokio::test]
async fn item_only_craft_preserves_clean_player_version_and_releases_full_hierarchy() {
    let (mut kernel, mut online, work) = fixture_with_dirty_ui(false);
    let binding = work.binding;
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut service = CraftingService::new();
    assert!(service.stage(work).is_ok());
    let mut token = 100;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            token += 1;
            service
                .poll_with(&mut online, &worker.handle, token, |c| {
                    kernel
                        .try_enqueue(c)
                        .map_err(|c| Box::new(TrySendError::Full(c)))
                })
                .unwrap();
            if service.completion.is_some() {
                break;
            }
            kernel.step().unwrap();
            while let Some(c) = kernel.take_player_snapshot_outcome() {
                service.accept_capture(c, 10_000).unwrap();
            }
            while let Some(c) = kernel.take_crafting_outcome() {
                service.accept_outcome(c).unwrap();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(service.take_completion().unwrap().committed);
    assert_eq!(online.baseline(binding.actor.0).unwrap().1, 3);
    assert!(online.critical_ready(&[binding.actor.0]).unwrap());
    assert_eq!(kernel.character(binding.actor).unwrap().revision(), 2);
    {
        let seen = backend.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(
            !seen[0]
                .snapshots
                .iter()
                .any(|s| s.object_id == binding.actor.0)
        );
    }
    assert!(kernel.inventory_item(SOURCE).is_none());
    assert_eq!(online.inventory_baselines(binding.actor.0).len(), 1);
    worker.handle.close();
    worker.task.await.unwrap();
}

#[tokio::test]
async fn salvage_commits_consumption_and_fresh_complete_bag_before_publishing() {
    let (mut kernel, mut online, work) = salvage_fixture();
    let binding = work.binding;
    let bag = work.generated[0].object_id;
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut service = CraftingService::new();
    assert!(service.stage(work).is_ok());
    let mut token = 100;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            token += 1;
            service
                .poll_with(&mut online, &worker.handle, token, |c| {
                    kernel
                        .try_enqueue(c)
                        .map_err(|c| Box::new(TrySendError::Full(c)))
                })
                .unwrap();
            if service.completion.is_some() {
                break;
            }
            kernel.step().unwrap();
            while let Some(c) = kernel.take_player_snapshot_outcome() {
                service.accept_capture(c, 10_000).unwrap();
            }
            while let Some(c) = kernel.take_crafting_outcome() {
                service.accept_outcome(c).unwrap();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let done = service.take_completion().unwrap();
    assert!(done.committed);
    assert!(kernel.inventory_item(TARGET).is_none());
    assert!(kernel.inventory_item(SOURCE).is_some());
    let saved = online
        .inventory_baselines(binding.actor.0)
        .into_iter()
        .find(|s| s.entity.object_id == bag)
        .unwrap();
    assert_eq!(saved.persisted_version, 1);
    assert_eq!(saved.entity.state.weenie_id, 123);
    for (id, expected) in [(91, 100), (92, 1), (131, 61), (170, 1)] {
        assert_eq!(
            saved
                .entity
                .state
                .properties
                .ints
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .value,
            expected
        );
    }
    assert!(saved.placement.is_some());
    assert_eq!(online.baseline(binding.actor.0).unwrap().1, 3);
    worker.handle.close();
    worker.task.await.unwrap();
}
