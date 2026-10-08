use bace_persistence::{
    AllegianceCommit, AllegianceOperation, AllegianceWrite, OperationOutcome, PlacementOperation,
    SaveAck, SaveSnapshot,
};
use bace_runtime::{
    allegiance_pending::{AllegianceResolution, PendingAllegianceSave},
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::{SaveBackend, SaveFailure, SaveWorkerConfig, spawn_save_worker},
};
use std::sync::{Arc, Mutex};
#[derive(Clone, Default)]
struct Backend {
    calls: Arc<Mutex<Vec<String>>>,
}
impl Backend {
    fn attempt(&self, id: &str) -> usize {
        let mut calls = self.calls.lock().unwrap();
        calls.push(id.into());
        calls.len()
    }
}
impl SaveBackend for Backend {
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        unreachable!("critical test")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        unreachable!("typed transaction only")
    }
    async fn placement(&self, op: &PlacementOperation) -> Result<OperationOutcome, SaveFailure> {
        match self.attempt(&op.operation_id) {
            1 => Err(SaveFailure::Storage {
                message: "lost commit reply".into(),
                uncertain: true,
            }),
            2 => Err(SaveFailure::Storage {
                message: "retry connection rejected".into(),
                uncertain: false,
            }),
            _ => Ok(OperationOutcome::AlreadyCommitted),
        }
    }
    async fn allegiance(&self, op: &AllegianceOperation) -> Result<AllegianceCommit, SaveFailure> {
        match self.attempt(&op.operation_id) {
            1 => Ok(AllegianceCommit::Committed {
                nodes: vec![],
                metadata: vec![],
                players: vec![],
            }),
            2 => Err(SaveFailure::Storage {
                message: "retry rejected".into(),
                uncertain: false,
            }),
            _ => Ok(AllegianceCommit::AlreadyCommitted),
        }
    }
}
async fn pause() {
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
}
#[tokio::test]
async fn ambiguous_placement_cannot_be_rejected_or_given_a_new_operation_id() {
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut pending = PendingPlacementSave::new(PlacementOperation {
        operation_id: "split:7:12".into(),
        snapshots: vec![SaveSnapshot {
            object_id: 2,
            mutation_revision: 8,
            expected_version: 5,
            bytes: vec![1],
        }],
        participants: vec![2],
        leases: vec![],
        changes: vec![],
        storage_views: vec![],
    })
    .unwrap();
    for attempt in 0..3 {
        pending.submit(&worker.handle).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Some(r) = pending.poll() {
                    break r;
                }
                pause().await;
            }
        })
        .await
        .unwrap();
        if attempt < 2 {
            assert!(matches!(result, PlacementResolution::Uncertain(_)));
        } else {
            match result {
                PlacementResolution::Committed(acks) => assert_eq!(
                    acks,
                    vec![SaveAck {
                        object_id: 2,
                        mutation_revision: 8,
                        persisted_version: 6
                    }]
                ),
                _ => panic!("exact retry must resolve"),
            }
        }
    }
    assert!(pending.submit(&worker.handle).is_err());
    assert_eq!(*backend.calls.lock().unwrap(), vec!["split:7:12"; 3]);
    drop(worker.handle);
    worker.task.await.unwrap();
}
#[tokio::test]
async fn lineage_receipt_mismatch_retains_exact_patch_until_durable_resolution() {
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut pending = PendingAllegianceSave::new(AllegianceOperation {
        operation_id: "allegiance:7:12".into(),
        nodes: vec![AllegianceWrite {
            character: 2,
            mutation_revision: 8,
            expected_version: 5,
            bytes: Some(vec![1]),
        }],
        metadata: vec![],
        players: vec![],
        leases: vec![],
    })
    .unwrap();
    for attempt in 0..3 {
        pending.submit(&worker.handle).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Some(r) = pending.poll() {
                    break r;
                }
                pause().await;
            }
        })
        .await
        .unwrap();
        if attempt < 2 {
            assert!(matches!(result, AllegianceResolution::Uncertain(_)));
        } else {
            match result {
                AllegianceResolution::Committed {
                    nodes,
                    metadata,
                    players,
                } => {
                    assert_eq!(
                        nodes,
                        vec![SaveAck {
                            object_id: 2,
                            mutation_revision: 8,
                            persisted_version: 6
                        }]
                    );
                    assert!(metadata.is_empty() && players.is_empty());
                }
                _ => panic!("exact retry must resolve"),
            }
        }
    }
    assert!(pending.submit(&worker.handle).is_err());
    assert_eq!(*backend.calls.lock().unwrap(), vec!["allegiance:7:12"; 3]);
    drop(worker.handle);
    worker.task.await.unwrap();
}
