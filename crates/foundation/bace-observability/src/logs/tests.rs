use super::*;
#[test]
fn exact_record_admission_keeps_text_and_returns_oversize_ownership() {
    let directory = tempfile::tempdir().unwrap();
    let store = LogStore::open(directory.path()).unwrap();
    let record = LogRecord {
        sequence: 0,
        unix_millis: 123,
        level: "INFO".into(),
        event: "chat.global".into(),
        message: "one\ntwo".into(),
    };
    store.try_record_exact(record).unwrap();
    for _ in 0..100 {
        if !store.recent(0).records.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let accepted = store.recent(0);
    assert_eq!(accepted.records[0].message, "one\ntwo");
    assert_eq!(accepted.records[0].unix_millis, 123);
    let oversized = LogRecord {
        sequence: 8,
        unix_millis: 456,
        level: "INFO".into(),
        event: "chat.global".into(),
        message: "x".repeat(EXACT_RECORD_BYTES + 1),
    };
    let returned = store.try_record_exact(oversized).unwrap_err();
    assert_eq!(returned.record.sequence, 8);
    assert_eq!(returned.record.unix_millis, 456);
    assert_eq!(returned.record.message.len(), EXACT_RECORD_BYTES + 1);
    assert_eq!(returned.kind, ExactLogErrorKind::Oversized);
    assert_eq!(store.recent(0).dropped, 0);
}

#[test]
fn rotation_keeps_four_bounded_files_and_utf8_records() {
    let directory = tempfile::tempdir().unwrap();
    let mut sink = Sink::open(directory.path()).unwrap();
    let record = LogRecord {
        sequence: 1,
        unix_millis: 0,
        level: "info".into(),
        event: "test".into(),
        message: bounded(&"é".repeat(RECORD_BYTES), RECORD_BYTES - 256),
    };
    assert!(record.message.len() <= RECORD_BYTES - 256);
    // Rotate deterministically without writing tens of megabytes.
    for _ in 0..8 {
        sink.bytes = FILE_BYTES;
        sink.write(&record).unwrap();
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), FILE_COUNT);
    for entry in fs::read_dir(directory.path()).unwrap() {
        let path = entry.unwrap().path();
        assert!(fs::metadata(&path).unwrap().len() <= FILE_BYTES);
        let text = fs::read_to_string(path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(parsed["message"], record.message);
    }
}

#[test]
fn producer_is_bounded_and_recent_batch_caps_delivery() {
    let directory = tempfile::tempdir().unwrap();
    let logs = LogStore::open(directory.path()).unwrap();
    for _ in 0..10_000 {
        logs.record("info", "test", &"x".repeat(4096));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while logs.recent(0).records.is_empty() && std::time::Instant::now() < deadline {
        thread::yield_now();
    }
    let batch = logs.recent(0);
    assert!(!batch.records.is_empty());
    assert!(batch.records.len() <= 128);
    let ring = logs.shared.ring.lock().unwrap();
    assert!(ring.records.len() <= RING_RECORDS);
    assert!(ring.bytes <= RING_BYTES);
}

use std::time::Duration;
#[test]
fn exact_log_distinguishes_pressure_and_closed_from_permanent_size_errors() {
    let (sender, receiver) = mpsc::sync_channel(1);
    let store = LogStore {
        send: Some(sender),
        shared: Arc::new(Shared {
            ring: Mutex::new(Ring::default()),
            dropped: AtomicU64::new(0),
            disk_degraded: AtomicBool::new(false),
        }),
        thread: None,
    };
    let record = LogRecord {
        sequence: 0,
        unix_millis: 123,
        level: "INFO".into(),
        event: "chat.global".into(),
        message: format!(
            "[CHAT][GLOBAL] Staff issued a world broadcast, \"{}\"",
            "\t".repeat(4096)
        ),
    };
    store.try_record_exact(record.clone()).unwrap();
    let pressure = store.try_record_exact(record.clone()).unwrap_err();
    assert_eq!(pressure.kind, ExactLogErrorKind::Full);
    assert_eq!(pressure.record.message, record.message);
    assert_eq!(receiver.recv().unwrap().message, record.message);
    drop(receiver);
    assert_eq!(
        store.try_record_exact(record).unwrap_err().kind,
        ExactLogErrorKind::Closed
    );
}
