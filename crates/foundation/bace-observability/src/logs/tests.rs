use super::*;

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
