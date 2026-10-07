use bace_dat::DatArchive;
use bace_dat_service::{
    DatabaseMetadata, DddCatalog, DddError, DddJob, DddLimits, DddSession, prepare_record,
};
use bace_runtime::dat_distribution::{DatPreparationWorker, DatSubmissionReason};
use bace_wire::{DddDatabase as Db, DddInterrogationResponse, DddIterationSet};
use std::{io::Write, sync::Arc};

const RECORD_ID: u32 = 0x01000001;
fn fixture() -> (tempfile::NamedTempFile, DatArchive, Vec<u8>) {
    // Synthetic B-tree/sector image following bace-dat/tests/archive.rs. It is
    // test scaffolding, not a distributed proprietary DAT or source of parity.
    let raw: Vec<u8> = (0..252).map(|n| n as u8).collect();
    let mut bytes = vec![0; 4096];
    let put = |bytes: &mut [u8], offset: usize, value: u32| {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes())
    };
    for (offset, value) in [
        (0x140, 0x5442),
        (0x144, 1024),
        (0x148, 4096),
        (0x14c, 1),
        (0x160, 1024),
        (1024, 2048),
        (1028 + 248, 1),
        (1028 + 256, RECORD_ID),
        (1028 + 260, 3072),
        (1028 + 264, 252),
        (1028 + 272, 2),
    ] {
        put(&mut bytes, offset, value);
    }
    bytes[3076..3076 + raw.len()].copy_from_slice(&raw);
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&bytes).unwrap();
    let archive = DatArchive::open(file.path()).unwrap();
    (file, archive, raw)
}
fn session(raw: &[u8], generation: u64) -> DddSession {
    let prepared = prepare_record(Db::Portal, RECORD_ID, 1, 2, raw, DddLimits::default()).unwrap();
    let catalog = Arc::new(
        DddCatalog::new(
            vec![
                DatabaseMetadata {
                    database: Db::Portal,
                    iteration: 2,
                    records: vec![prepared.metadata()],
                },
                DatabaseMetadata {
                    database: Db::Language,
                    iteration: 1,
                    records: vec![],
                },
            ],
            DddLimits::default(),
        )
        .unwrap(),
    );
    let mut session = DddSession::new(catalog, DddLimits::default(), true, generation).unwrap();
    session
        .begin(&DddInterrogationResponse {
            client_language: 1,
            with_keys: vec![
                DddIterationSet {
                    dat_file_type: 0,
                    dat_file_id: 1,
                    iterations: 1,
                    runs: vec![1],
                },
                DddIterationSet {
                    dat_file_type: 1,
                    dat_file_id: 3,
                    iterations: 1,
                    runs: vec![1],
                },
            ],
            trailing_bytes: 0,
        })
        .unwrap();
    session
}

#[test]
fn bounded_preparation_returns_full_job_ownership_and_drains_completed_work() {
    let (_file, archive, raw) = fixture();
    let mut session = session(&raw, 9);
    let job = session.take_job().unwrap();
    let mut worker =
        DatPreparationWorker::spawn(vec![(Db::Portal, archive)], DddLimits::default(), 1).unwrap();
    let observer = worker.observer();
    worker.try_submit(job).unwrap();
    let rejected = worker.try_submit(job).unwrap_err();
    assert_eq!(rejected.job, job);
    assert_eq!(rejected.reason, DatSubmissionReason::Full);
    let mut drain = worker.shutdown();
    assert!(!drain.panicked);
    assert!(drain.unrecovered_jobs.is_empty());
    assert_eq!(drain.completions.len(), 1);
    assert_eq!(
        drain
            .completions
            .pop()
            .unwrap()
            .apply(&mut session)
            .unwrap()[..4],
        0xf7e2u32.to_le_bytes()
    );
    assert_eq!(session.pending_records(), 0);
    let counts = observer.counts();
    assert_eq!(
        (
            counts.submitted,
            counts.prepared,
            counts.delivered,
            counts.abandoned
        ),
        (1, 1, 1, 0)
    );
}

#[test]
fn stale_worker_completion_does_not_mutate_reconnected_session() {
    let (_file, archive, raw) = fixture();
    let mut old = session(&raw, 9);
    let job = old.take_job().unwrap();
    let mut new = session(&raw, 10);
    let new_job = new.take_job().unwrap();
    let mut worker =
        DatPreparationWorker::spawn(vec![(Db::Portal, archive)], DddLimits::default(), 1).unwrap();
    worker.try_submit(job).unwrap();
    let completion = worker.shutdown().completions.pop().unwrap();
    assert!(matches!(
        completion.apply(&mut new),
        Err(DddError::StaleCompletion)
    ));
    assert_eq!(new.pending_records(), 1);
    new.retry_job(new_job).unwrap();
    assert_eq!(new.take_job(), Some(new_job));
}

#[test]
fn missing_record_and_metadata_mismatch_are_explicit_retryable_errors() {
    let (_file, archive, raw) = fixture();
    let mut session = session(&raw, 9);
    let job = session.take_job().unwrap();
    let mut worker =
        DatPreparationWorker::spawn(vec![(Db::Portal, archive)], DddLimits::default(), 2).unwrap();
    let missing = DddJob {
        record: bace_dat_service::RecordMetadata {
            object_id: RECORD_ID + 1,
            ..job.record
        },
        ..job
    };
    worker.try_submit(missing).unwrap();
    let mismatch = DddJob {
        record: bace_dat_service::RecordMetadata {
            iteration: 1,
            ..job.record
        },
        ..job
    };
    worker.try_submit(mismatch).unwrap();
    let drain = worker.shutdown();
    assert_eq!(drain.completions.len(), 2);
    assert!(matches!(
        drain.completions[0].result,
        Err(DddError::NotFound)
    ));
    assert!(matches!(
        drain.completions[1].result,
        Err(DddError::PreparedRecordMismatch)
    ));
    // Neither error can clear the actual session's original in-flight job.
    assert_eq!(session.pending_records(), 1);
    session.retry_job(job).unwrap();
}

#[test]
fn drop_accounts_for_results_instead_of_claiming_session_success() {
    let (_file, archive, raw) = fixture();
    let mut session = session(&raw, 9);
    let job = session.take_job().unwrap();
    let mut worker =
        DatPreparationWorker::spawn(vec![(Db::Portal, archive)], DddLimits::default(), 1).unwrap();
    let observer = worker.observer();
    worker.try_submit(job).unwrap();
    drop(worker);
    assert_eq!(observer.counts().abandoned, 1);
    assert_eq!(observer.counts().delivered, 0);
    assert_eq!(session.pending_records(), 1);
    session.retry_job(job).unwrap();
}
