use super::*;
mod fixture;
mod output;
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
    async fn world_placement(
        &self,
        op: &bace_persistence::WorldPlacementOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.placement(&op.inventory).await
    }
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("wrong lane")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("unfenced write")
    }
}
#[tokio::test]
async fn split_preserves_unsaved_ui_and_retries_exact_bytes_before_owner_adoption() {
    let (mut kernel, mut online, work) = fixture();
    let actor = work.binding.actor;
    let binding = work.binding;
    assert!(
        kernel
            .read_player_operation_snapshot(
                binding,
                PlayerSnapshotOperation::Inventory(work.operation.ticket.operation + 1),
                work.operation.actor_revision
            )
            .is_err()
    );
    let backend = Backend::default();
    backend.answers.lock().unwrap().extend([1, 2, 0]);
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut service = InventoryService::new();
    assert!(service.stage(work).is_ok());
    let mut pressure = 3;
    let mut uncertain = 0;
    let mut token = 100;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            token += 1;
            let result = service.poll_with(&mut online, &worker.handle, token, |command| {
                if matches!(command, Command::Inventory(_)) && pressure > 0 {
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
            while let Some(outcome) = kernel.take_inventory_outcome() {
                service.accept_outcome(outcome).unwrap();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(uncertain, 2);
    assert_eq!(pressure, 0);
    let completion = service.take_completion().unwrap();
    assert!(completion.committed);
    output::assert_complete_output(&completion);
    assert!(!service.requires_drain());
    assert!(kernel.inventory_item(SOURCE).unwrap().stack == 3);
    assert_eq!(kernel.inventory_item(FRESH).unwrap().revision, 1);
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
        assert_eq!(saved.player.entity.mutation_revision, 3);
    }
    {
        let rows = backend.seen.lock().unwrap();
        let new = bace_storage_codec::ItemSaveV5::decode(
            &rows[0]
                .snapshots
                .iter()
                .find(|s| s.object_id == FRESH.0)
                .unwrap()
                .bytes,
        )
        .unwrap();
        let old = bace_storage_codec::ItemSaveV5::decode(
            &rows[0]
                .snapshots
                .iter()
                .find(|s| s.object_id == SOURCE.0)
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert!(
            new.entity.state.properties.strings.is_empty(),
            "split must use fresh factory data"
        );
        assert_eq!(
            old.entity.state.properties.strings[0].value,
            "instance-only mutation"
        );
        assert!(
            matches!(new.placement,bace_storage_codec::ItemPlacementV2::Contained{container,slot:2,equipped:0,..}if container==actor.0)
        );
        assert_eq!(kernel.inventory_item(FRESH).unwrap().stack, 2);
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
    let mut service = InventoryService::new();
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
    let mut outcome = kernel.take_inventory_outcome().unwrap();
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
