use bace_dat_service::*;
use bace_wire::{DddDatabase as Db, DddInterrogationResponse, DddIterationSet, DddRequestData};
use std::{io::Read, sync::Arc};

fn record(database: Db, id: u32, kind: u32, iteration: u32, raw: &[u8]) -> PreparedRecord {
    prepare_record(database, id, kind, iteration, raw, DddLimits::default()).unwrap()
}
fn response(portal: DddIterationSet) -> DddInterrogationResponse {
    DddInterrogationResponse {
        client_language: 1,
        with_keys: vec![
            portal,
            DddIterationSet {
                dat_file_type: 1,
                dat_file_id: 3,
                iterations: 1,
                runs: vec![1],
            },
        ],
        trailing_bytes: 0,
    }
}
fn portal(iterations: u32, runs: Vec<i32>) -> DddIterationSet {
    DddIterationSet {
        dat_file_type: 0,
        dat_file_id: 1,
        iterations,
        runs,
    }
}
fn catalog(record: &PreparedRecord) -> Arc<DddCatalog> {
    Arc::new(
        DddCatalog::new(
            vec![
                DatabaseMetadata {
                    database: Db::Portal,
                    iteration: 2,
                    records: vec![record.metadata()],
                },
                DatabaseMetadata {
                    database: Db::Language,
                    iteration: 1,
                    records: vec![],
                },
                DatabaseMetadata {
                    database: Db::Cell,
                    iteration: 1,
                    records: vec![
                        record_fn(0x1234fffe, 2).metadata(),
                        record_fn(0x1234ffff, 1).metadata(),
                    ],
                },
            ],
            DddLimits::default(),
        )
        .unwrap(),
    )
}
fn record_fn(id: u32, kind: u32) -> PreparedRecord {
    record(Db::Cell, id, kind, 1, &[9, 8, 7, 6])
}

#[test]
fn compression_prefix_threshold_and_independent_inflater() {
    let raw = vec![42; 8192];
    let prepared = record(Db::Portal, 1, 1, 1, &raw);
    assert!(prepared.metadata().compressed);
    assert_eq!(&prepared.payload()[..4], &8192u32.to_le_bytes());
    // Different decoder API verifies framing and restored bytes, not a wire
    // self-roundtrip or a claim of matching .NET's exact compressed bitstream.
    let mut restored = Vec::new();
    flate2::read::ZlibDecoder::new(&prepared.payload()[4..])
        .read_to_end(&mut restored)
        .unwrap();
    assert_eq!(restored, raw);
    for raw in [vec![], vec![1], vec![1, 2, 3, 4]] {
        let prepared = record(Db::Portal, 1, 1, 1, &raw);
        assert!(!prepared.metadata().compressed);
        assert_eq!(prepared.payload(), raw);
    }
}

#[test]
fn disabled_and_newer_clients_never_receive_fabricated_patch_success() {
    let prepared = record(Db::Portal, 1, 1, 2, &[1; 4]);
    let mut session = DddSession::new(catalog(&prepared), DddLimits::default(), false, 9).unwrap();
    assert!(matches!(
        session.begin(&response(portal(1, vec![1]))),
        Err(DddError::Disabled)
    ));
    assert_eq!(session.pending_records(), 0);
    assert!(matches!(
        session.begin(&response(portal(3, vec![-3, 1]))),
        Err(DddError::NewerClient)
    ));
    assert!(matches!(
        session.begin(&response(portal(2, vec![-2, 1]))),
        Ok(DddStart::UpToDate(_))
    ));
}

#[test]
fn patch_work_is_retained_on_failure_and_completion_is_generation_fenced() {
    let prepared = record(Db::Portal, 1, 1, 2, &[1; 8192]);
    let mut session = DddSession::new(catalog(&prepared), DddLimits::default(), true, 9).unwrap();
    let start = session.begin(&response(portal(1, vec![1]))).unwrap();
    assert!(
        matches!(start, DddStart::Patch { queued_records: 1, transfer_bytes, .. } if transfer_bytes == prepared.metadata().transfer_size)
    );
    assert!(matches!(
        session.acknowledge_end(),
        Err(DddError::InvalidState)
    ));
    let job = session.take_job().unwrap();
    assert!(session.take_job().is_none());
    assert!(matches!(
        session.complete_job(
            DddJob {
                generation: 8,
                ..job
            },
            &prepared
        ),
        Err(DddError::StaleCompletion)
    ));
    session.retry_job(job).unwrap();
    assert_eq!(session.take_job(), Some(job));
    let wrong = record(Db::Portal, 2, 1, 2, &[1; 4]);
    assert!(matches!(
        session.complete_job(job, &wrong),
        Err(DddError::PreparedRecordMismatch)
    ));
    assert_eq!(session.pending_records(), 1);
    let bytes = session.complete_job(job, &prepared).unwrap();
    assert_eq!(&bytes[..4], &0xf7e2u32.to_le_bytes());
    assert_eq!(session.pending_records(), 0);
    assert!(session.acknowledge_end().unwrap().is_some());
    assert!(session.acknowledge_end().unwrap().is_none());
}

#[test]
fn bounded_on_demand_landblock_includes_info_and_missing_is_explicit() {
    let prepared = record(Db::Portal, 1, 1, 2, &[1; 4]);
    let limits = DddLimits {
        max_pending_records: 2,
        ..Default::default()
    };
    let mut session = DddSession::new(catalog(&prepared), limits, true, 9).unwrap();
    session.begin(&response(portal(2, vec![-2, 1]))).unwrap();
    let request = DddRequestData {
        resource_type: 1,
        object_id: 0x1234ffff,
    };
    session.request(request).unwrap();
    assert_eq!(session.pending_records(), 2);
    assert_eq!(session.take_job().unwrap().record.object_id, 0x1234fffe);
    assert!(matches!(session.request(request), Err(DddError::Capacity)));
    assert_eq!(session.pending_records(), 2);
    assert!(matches!(
        session.request(DddRequestData {
            object_id: 4,
            ..request
        }),
        Err(DddError::NotFound)
    ));
    assert!(matches!(
        session.request(DddRequestData {
            resource_type: 99,
            ..request
        }),
        Err(DddError::UnsupportedRequest)
    ));
}

#[test]
fn malformed_iteration_runs_and_missing_required_databases_are_rejected_atomically() {
    let prepared = record(Db::Portal, 1, 1, 2, &[1; 4]);
    let mut session = DddSession::new(catalog(&prepared), DddLimits::default(), true, 9).unwrap();
    for set in [
        portal(2, vec![1, 1]),
        portal(2, vec![-2]),
        portal(1, vec![-1, 1]),
        portal(1, vec![0]),
    ] {
        assert!(matches!(
            session.begin(&response(set)),
            Err(DddError::InvalidIterations)
        ));
        assert_eq!(session.pending_records(), 0);
    }
    assert!(matches!(
        session.begin(&DddInterrogationResponse {
            client_language: 1,
            with_keys: vec![],
            trailing_bytes: 0
        }),
        Err(DddError::MissingDatabase)
    ));
    // A claimed last iteration does not prove possession of the earlier one.
    assert!(matches!(
        session.begin(&response(portal(1, vec![2]))),
        Ok(DddStart::Patch { .. })
    ));
}
