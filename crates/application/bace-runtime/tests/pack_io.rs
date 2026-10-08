use bace_runtime::pack_io::{PackCompletion, PackIoWorker, PackJob};
use bace_storage_codec::{PackKey, PackLimits, PackLookup, PackManifest, PackRecord};
use std::sync::{Arc, atomic::AtomicBool};

fn run(directory: &std::path::Path, job: PackJob, pressure: bool) -> PackCompletion {
    let worker = PackIoWorker::start(
        directory.to_path_buf(),
        1,
        Arc::new(AtomicBool::new(pressure)),
    )
    .unwrap();
    assert!(worker.try_submit(job).is_ok());
    let mut completed = worker.shutdown().unwrap();
    assert_eq!(completed.len(), 1);
    completed.remove(0)
}

#[test]
fn pack_worker_preserves_readers_limits_and_retryable_maintenance() {
    let directory = tempfile::tempdir().unwrap();
    let key = PackKey {
        namespace: 1,
        id: 1,
    };
    let record = |byte| PackRecord {
        key,
        schema: 1,
        value: Some(vec![byte]),
    };
    let base =
        bace_storage_codec::compile_pack(directory.path(), [Ok(record(1))], PackLimits::default())
            .unwrap();
    let original = PackManifest {
        version: 1,
        generation: 1,
        base,
        deltas: vec![],
    };
    let opened = run(
        directory.path(),
        PackJob::Open {
            manifest: original.clone(),
        },
        false,
    )
    .result
    .unwrap();
    let PackLookup::Record(old_record) = opened.generation.lookup(key).unwrap() else {
        panic!("record")
    };
    let second = run(
        directory.path(),
        PackJob::Delta {
            current: original,
            records: vec![record(2)],
        },
        false,
    )
    .result
    .unwrap();
    let third = run(
        directory.path(),
        PackJob::Delta {
            current: second.manifest,
            records: vec![record(3)],
        },
        false,
    )
    .result
    .unwrap();
    let full = run(
        directory.path(),
        PackJob::Delta {
            current: third.manifest.clone(),
            records: vec![record(4)],
        },
        false,
    );
    assert!(full.result.err().unwrap().contains("three active packs"));
    let postponed = run(
        directory.path(),
        PackJob::Compact {
            current: third.manifest.clone(),
        },
        true,
    );
    assert!(postponed.result.err().unwrap().contains("save pressure"));
    let compacted = run(directory.path(), postponed.job, false).result.unwrap();
    assert!(compacted.manifest.deltas.is_empty());
    let PackLookup::Record(record) = compacted.generation.lookup(key).unwrap() else {
        panic!("record")
    };
    assert_eq!(record.bytes(), &[3]);
    assert_eq!(old_record.bytes(), &[1]);
    let added = run(
        directory.path(),
        PackJob::Delta {
            current: compacted.manifest,
            records: vec![PackRecord {
                key,
                schema: 1,
                value: None,
            }],
        },
        false,
    )
    .result
    .unwrap();
    assert!(matches!(
        added.generation.lookup(key).unwrap(),
        PackLookup::Tombstone
    ));
}
