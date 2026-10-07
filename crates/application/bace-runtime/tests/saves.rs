use bace_persistence::{DirtySaves, OperationOutcome, SaveAck, SaveSnapshot};
use bace_runtime::saves::{
    SaveBackend, SaveFailure, SaveSubmitError, SaveWorkerConfig, WriteOutcome, spawn_save_worker,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{sync::Semaphore, time::Instant};

#[derive(Clone)]
struct Backend {
    log: Arc<Mutex<Vec<(bool, u32)>>>,
    first: Arc<AtomicBool>,
    entered: Arc<Semaphore>,
    release: Arc<Semaphore>,
    gate: bool,
    hang: Option<u32>,
}
impl Backend {
    fn new(gate: bool, hang: Option<u32>) -> Self {
        Self {
            log: Arc::new(Mutex::new(Vec::new())),
            first: Arc::new(AtomicBool::new(true)),
            entered: Arc::new(Semaphore::new(0)),
            release: Arc::new(Semaphore::new(0)),
            gate,
            hang,
        }
    }
    async fn write(&self, valuable: bool, snapshots: &[SaveSnapshot]) -> Vec<SaveAck> {
        {
            self.log
                .lock()
                .unwrap()
                .push((valuable, snapshots[0].object_id));
        }
        if self.gate && self.first.swap(false, Ordering::SeqCst) {
            self.entered.add_permits(1);
            self.release.acquire().await.unwrap().forget();
        }
        if self.hang == Some(snapshots[0].object_id) {
            std::future::pending::<()>().await;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
        snapshots
            .iter()
            .map(|s| SaveAck {
                object_id: s.object_id,
                mutation_revision: s.mutation_revision,
                persisted_version: s.expected_version + 1,
            })
            .collect()
    }
}
impl SaveBackend for Backend {
    async fn offline(
        &self,
        event: &bace_persistence::OfflineXpEvent,
        _lease: bace_persistence::CharacterLease,
    ) -> Result<bace_persistence::XpReceipt, SaveFailure> {
        self.write(false, &[snapshot(event.target_character)]).await;
        Ok(bace_persistence::XpReceipt {
            event_id: event.event_id.clone(),
            cached_xp: event.amount,
            newly_applied: true,
        })
    }

    async fn routine(&self, snapshots: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        Ok(self.write(false, snapshots).await)
    }
    async fn valuable(
        &self,
        _: &str,
        snapshots: &[SaveSnapshot],
    ) -> Result<OperationOutcome, SaveFailure> {
        Ok(OperationOutcome::Committed(
            self.write(true, snapshots).await,
        ))
    }
}
fn snapshot(id: u32) -> SaveSnapshot {
    SaveSnapshot {
        object_id: id,
        mutation_revision: 1,
        expected_version: 0,
        bytes: vec![1],
    }
}
fn config() -> SaveWorkerConfig {
    SaveWorkerConfig {
        routine_capacity: 16,
        valuable_capacity: 16,
        operation_timeout: Duration::from_secs(1),
        ..Default::default()
    }
}

#[tokio::test]
async fn blocked_save_adapter_does_not_own_or_stall_simulation() {
    use bace_runtime::simulation::{SimulationConfig, SimulationWorker};
    let backend = Backend::new(true, None);
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let started = Instant::now();
    let mut ticket = worker.handle.try_routine(&[snapshot(1)], started).unwrap();
    tokio::time::timeout(Duration::from_secs(2), backend.entered.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();

    let kernel = bace_simulation::synthetic_scenario(100, 1000).unwrap();
    let simulation = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            tick_limit: Some(5),
            ..SimulationConfig::default()
        },
    )
    .unwrap();
    let report = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking(move || simulation.wait()),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(report.ticks, 5);
    assert!(matches!(
        ticket.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));

    // The storage adapter remains blocked for at least 500 ms and until after
    // the simulation finishes. This gate avoids a fragile relative-speed test.
    tokio::time::sleep_until(started + Duration::from_millis(500)).await;
    backend.release.add_permits(1);
    assert!(ticket.await.unwrap().result.is_ok());
    drop(worker.handle);
    assert_eq!(worker.task.await.unwrap().routine_completed, 1);
}

#[tokio::test]
async fn continuous_critical_and_new_routine_arrivals_cannot_starve_old_dirty_state() {
    let backend = Backend::new(true, None);
    let worker = spawn_save_worker(backend.clone(), config()).unwrap();
    let first = worker
        .handle
        .try_valuable("first", &[snapshot(1000)])
        .unwrap();
    backend.entered.acquire().await.unwrap().forget();
    let mut initial = Vec::new();
    for id in 1001..1017 {
        initial.push(
            worker
                .handle
                .try_valuable(&format!("initial-{id}"), &[snapshot(id)])
                .unwrap(),
        );
    }
    let old = Instant::now() - Duration::from_secs(30);
    let oldest = worker.handle.try_routine(&[snapshot(900)], old).unwrap();
    let critical_handle = worker.handle.clone();
    let critical_producer = tokio::spawn(async move {
        for id in 1100..1180 {
            loop {
                match critical_handle.try_valuable(&format!("continued-{id}"), &[snapshot(id)]) {
                    Ok(ticket) => {
                        assert!(ticket.await.unwrap().result.is_ok());
                        break;
                    }
                    Err(SaveSubmitError::Full) => tokio::task::yield_now().await,
                    Err(error) => panic!("unexpected submission failure: {error}"),
                }
            }
        }
    });
    let routine_handle = worker.handle.clone();
    let hot_producer = tokio::spawn(async move {
        for revision in 1..=40 {
            let mut hot = snapshot(1);
            hot.mutation_revision = revision;
            let ticket = loop {
                match routine_handle
                    .try_routine(&[hot.clone()], Instant::now() - Duration::from_secs(5))
                {
                    Ok(ticket) => break ticket,
                    Err(SaveSubmitError::Full) => tokio::task::yield_now().await,
                    Err(error) => panic!("unexpected submission failure: {error}"),
                }
            };
            assert!(ticket.await.unwrap().result.is_ok());
        }
    });
    backend.release.add_permits(1);
    let report = tokio::time::timeout(Duration::from_secs(1), oldest)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.dirty_since, Some(old));
    assert!(matches!(report.result, Ok(WriteOutcome::Routine(_))));
    let log = backend.log.lock().unwrap().clone();
    let index = log.iter().position(|entry| *entry == (false, 900)).unwrap();
    assert!(
        index <= 4,
        "critical burst must hand off to oldest due routine: {log:?}"
    );
    assert!(first.await.unwrap().result.is_ok());
    for ticket in initial {
        assert!(ticket.await.unwrap().result.is_ok());
    }
    critical_producer.await.unwrap();
    hot_producer.await.unwrap();
    drop(worker.handle);
    let summary = worker.task.await.unwrap();
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.routine_completed, 41);
    assert_eq!(summary.valuable_completed, 97);
}

#[tokio::test]
async fn critical_capacity_cannot_consume_reserved_routine_capacity() {
    let backend = Backend::new(true, None);
    let config = SaveWorkerConfig {
        valuable_capacity: 1,
        valuable_bytes: 1,
        routine_capacity: 1,
        routine_bytes: 1,
        ..config()
    };
    let worker = spawn_save_worker(backend.clone(), config).unwrap();
    let critical = worker.handle.try_valuable("one", &[snapshot(1)]).unwrap();
    backend.entered.acquire().await.unwrap().forget();
    assert!(matches!(
        worker.handle.try_valuable("two", &[snapshot(2)]),
        Err(SaveSubmitError::Full)
    ));
    let routine = worker
        .handle
        .try_routine(&[snapshot(3)], Instant::now() - Duration::from_secs(5))
        .unwrap();
    backend.release.add_permits(1);
    assert!(critical.await.unwrap().result.is_ok());
    assert!(routine.await.unwrap().result.is_ok());
    drop(worker.handle);
    let summary = worker.task.await.unwrap();
    assert_eq!(summary.routine_completed, 1);
}

#[tokio::test]
async fn timeout_reports_uncertainty_preserves_owner_age_and_does_not_block_next_save() {
    let backend = Backend::new(false, Some(7));
    let worker = spawn_save_worker(
        backend,
        SaveWorkerConfig {
            operation_timeout: Duration::from_millis(20),
            ..config()
        },
    )
    .unwrap();
    let mut owner = DirtySaves::new(2);
    owner.mark_at(snapshot(7), Duration::ZERO).unwrap();
    let sent = owner.due(Duration::from_secs(5));
    let original = Instant::now() - Duration::from_secs(10);
    let failed = worker.handle.try_routine(&sent, original).unwrap();
    let next = worker
        .handle
        .try_routine(&[snapshot(8)], Instant::now() - Duration::from_secs(5))
        .unwrap();
    let failure = failed.await.unwrap();
    assert!(matches!(failure.result, Err(SaveFailure::Timeout)));
    assert_eq!(failure.dirty_since, Some(original));
    assert!(failure.started_late_by >= Duration::from_secs(5));
    assert!(!owner.is_clean());
    assert_eq!(owner.dirty_since(7), Some(Duration::ZERO));
    // Do not release/retry owner in-flight state until uncertain durable outcome is reconciled.
    assert!(next.await.unwrap().result.is_ok());
    drop(worker.handle);
    let summary = worker.task.await.unwrap();
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.routine_completed, 1);
}

#[tokio::test]
async fn offline_and_routine_lanes_both_progress_under_critical_pressure() {
    use bace_persistence::{CharacterLease, OfflineXpEvent, OwnershipState};
    let backend = Backend::new(true, None);
    let worker = spawn_save_worker(backend.clone(), config()).unwrap();
    let first = worker
        .handle
        .try_valuable("gate", &[snapshot(9000)])
        .unwrap();
    backend.entered.acquire().await.unwrap().forget();
    let old = Instant::now() - Duration::from_secs(30);
    let mut routine = Vec::new();
    let mut offline = Vec::new();
    let mut critical = Vec::new();
    for id in 1..=12 {
        routine.push(
            worker
                .handle
                .try_routine(&[snapshot(100 + id)], old)
                .unwrap(),
        );
        let event = OfflineXpEvent {
            event_id: format!("xp-{id}"),
            source_character: 1,
            target_character: 200 + id,
            amount: 10,
        };
        let lease = CharacterLease {
            character_id: 200 + id,
            epoch: 0,
            state: OwnershipState::Offline,
        };
        offline.push(worker.handle.try_offline(&event, lease, old).unwrap());
        critical.push(
            worker
                .handle
                .try_valuable(&format!("valuable-{id}"), &[snapshot(300 + id)])
                .unwrap(),
        );
    }
    let producer = worker.handle.clone();
    let sustained = tokio::spawn(async move {
        for id in 1..=80 {
            loop {
                match producer.try_valuable(&format!("continued-{id}"), &[snapshot(500 + id)]) {
                    Ok(t) => {
                        assert!(t.await.unwrap().result.is_ok());
                        break;
                    }
                    Err(SaveSubmitError::Full) => tokio::task::yield_now().await,
                    Err(e) => panic!("{e}"),
                }
            }
        }
    });
    backend.release.add_permits(1);
    for ticket in routine.into_iter().chain(offline) {
        assert!(
            tokio::time::timeout(Duration::from_secs(2), ticket)
                .await
                .unwrap()
                .unwrap()
                .result
                .is_ok()
        );
    }
    assert!(first.await.unwrap().result.is_ok());
    for ticket in critical {
        assert!(ticket.await.unwrap().result.is_ok())
    }
    sustained.await.unwrap();
    let log = backend.log.lock().unwrap().clone();
    assert!(log.iter().position(|r| r.1 == 101).unwrap() <= 4);
    assert!(log.iter().position(|r| r.1 == 201).unwrap() <= 9);
    drop(log);
    worker.handle.close();
    drop(worker.handle);
    let summary = worker.task.await.unwrap();
    assert_eq!(summary.routine_completed, 12);
    assert_eq!(summary.offline_completed, 12);
    assert_eq!(summary.failed, 0);
}

#[tokio::test]
async fn dedicated_persistence_thread_runs_while_caller_runtime_is_not_polled() {
    use bace_runtime::persistence_thread::PersistenceThread;
    let backend = Backend::new(false, None);
    let built_on = Arc::new(Mutex::new(None));
    let record = built_on.clone();
    let thread = PersistenceThread::spawn(config(), move || async move {
        *record.lock().unwrap() = Some(std::thread::current().id());
        Ok(backend)
    })
    .await
    .unwrap();
    assert_ne!(thread.thread_id, std::thread::current().id());
    assert_eq!(*built_on.lock().unwrap(), Some(thread.thread_id));
    let mut ticket = thread
        .handle
        .try_routine(&[snapshot(1)], Instant::now())
        .unwrap();
    // This tokio test uses a current-thread runtime. No caller executor is polled during this sleep.
    std::thread::sleep(Duration::from_millis(100));
    assert!(ticket.try_recv().unwrap().result.is_ok());
    let remaining_clone = thread.handle.clone();
    let summary = tokio::time::timeout(Duration::from_secs(2), thread.drain())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(summary.routine_completed, 1);
    assert!(matches!(
        remaining_clone.try_routine(&[snapshot(2)], Instant::now()),
        Err(SaveSubmitError::Closed)
    ));
}
