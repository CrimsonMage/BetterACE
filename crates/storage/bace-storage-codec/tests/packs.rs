use bace_storage_codec::*;
use sha2::{Digest, Sha256};
use std::path::Path;

fn key(id: u64) -> PackKey {
    PackKey { namespace: 1, id }
}
fn record(id: u64, value: Option<&[u8]>) -> Result<PackRecord, PackError> {
    Ok(PackRecord {
        key: key(id),
        schema: 2,
        value: value.map(<[u8]>::to_vec),
    })
}
fn compile(dir: &Path, records: Vec<Result<PackRecord, PackError>>) -> PackDescriptor {
    compile_pack(dir, records, PackLimits::default()).unwrap()
}
fn bytes(found: PackLookup) -> RecordHandle {
    match found {
        PackLookup::Record(record) => record,
        _ => panic!("expected record"),
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

#[test]
fn independent_layout_golden_and_repeat_compile_are_deterministic() {
    // Independently calculated with Python struct.pack('<H6xQ'),
    // struct.pack('<HH4xQQ') and hashlib.sha256; format fixture, not ACE parity.
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = compile(a.path(), vec![record(7, Some(b"abc"))]);
    assert_eq!(
        first.file_name,
        "5fde3984c6bde9fd6aa6918b8b924e0496b9f03ef334efcda6c92b087d1020d6.bace"
    );
    let second = compile(b.path(), vec![record(7, Some(b"abc"))]);
    assert_eq!(first, second);
    let raw = std::fs::read(a.path().join(&first.file_name)).unwrap();
    assert_eq!(
        raw,
        std::fs::read(b.path().join(&second.file_name)).unwrap()
    );
    assert_eq!(&raw[..8], b"BACEPACK");
    assert_eq!(raw.len(), 283);
    assert_eq!(&raw[128..131], b"abc");
    assert_eq!(u64_at(&raw, 32), 203);
    assert_eq!(
        compile_pack(a.path(), [record(7, Some(b"abc"))], PackLimits::default()).unwrap(),
        first
    );
    let map = MappedPack::open(a.path(), &first, PackLimits::default()).unwrap();
    assert_eq!(bytes(map.lookup(key(7)).unwrap()).bytes(), b"abc");
}

#[test]
fn newest_overlay_wins_tombstones_hide_base_and_old_handles_stay_valid() {
    let dir = tempfile::tempdir().unwrap();
    let limits = PackLimits::default();
    let base = compile(
        dir.path(),
        vec![record(1, Some(b"original")), record(2, Some(b"delete"))],
    );
    let base_map = MappedPack::open(dir.path(), &base, limits).unwrap();
    let old = bytes(base_map.lookup(key(1)).unwrap());
    let delta = compile(
        dir.path(),
        vec![
            record(1, Some(b"replacement")),
            record(2, None),
            record(3, Some(b"new")),
        ],
    );
    let manifest = PackManifest {
        version: 1,
        generation: 42,
        base,
        deltas: vec![delta],
    };
    let manifest_path = write_manifest(dir.path(), &manifest, limits).unwrap();
    assert_eq!(load_manifest(&manifest_path, limits).unwrap(), manifest);
    let generation = manifest.open(dir.path(), limits).unwrap();
    assert_eq!(generation.revision(), 42);
    assert_eq!(
        generation.newest_delta_keys().unwrap(),
        vec![key(1), key(2), key(3)]
    );
    assert_eq!(
        bytes(generation.lookup(key(1)).unwrap()).bytes(),
        b"replacement"
    );
    assert!(matches!(
        generation.lookup(key(2)).unwrap(),
        PackLookup::Tombstone
    ));
    assert_eq!(bytes(generation.lookup(key(3)).unwrap()).bytes(), b"new");
    assert!(matches!(
        generation.lookup(key(4)).unwrap(),
        PackLookup::Missing
    ));
    drop(base_map);
    drop(generation);
    assert_eq!(old.bytes(), b"original");
    assert_eq!(old.schema(), 2);
    // A fresh restart uses saved immutable descriptors, without compilation.
    let restarted = load_manifest(&manifest_path, limits)
        .unwrap()
        .open(dir.path(), limits)
        .unwrap();
    assert_eq!(bytes(restarted.lookup(key(3)).unwrap()).bytes(), b"new");
}

#[test]
fn changed_keys_after_compaction_are_only_the_new_publication_delta() {
    let dir = tempfile::tempdir().unwrap();
    let limits = PackLimits::default();
    let base = compile(
        dir.path(),
        vec![record(1, Some(b"one")), record(2, Some(b"two"))],
    );
    let older = compile(dir.path(), vec![record(1, Some(b"replaced"))]);
    let newest = compile(dir.path(), vec![record(2, None)]);
    let full = PackManifest {
        version: 1,
        generation: 3,
        base,
        deltas: vec![older, newest],
    };
    let generation = full.open(dir.path(), limits).unwrap();
    let compacted_base = generation
        .compact(
            dir.path(),
            limits,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
    let compacted = PackManifest {
        version: 1,
        generation: 4,
        base: compacted_base.clone(),
        deltas: vec![],
    };
    assert!(
        compacted
            .open(dir.path(), limits)
            .unwrap()
            .newest_delta_keys()
            .unwrap()
            .is_empty()
    );
    let publication = compile(dir.path(), vec![record(1, None), record(3, Some(b"new"))]);
    let accepted = PackManifest {
        version: 1,
        generation: 5,
        base: compacted_base,
        deltas: vec![publication],
    };
    assert_eq!(
        accepted
            .open(dir.path(), limits)
            .unwrap()
            .newest_delta_keys()
            .unwrap(),
        vec![key(1), key(3)]
    );
}

#[test]
fn scans_are_ordered_cursor_bounded_and_preserve_empty_values() {
    let dir = tempfile::tempdir().unwrap();
    let descriptor = compile_pack(
        dir.path(),
        (0..100).map(|i| record(i, Some(b"x"))),
        PackLimits::default(),
    )
    .unwrap();
    let map = MappedPack::open(dir.path(), &descriptor, PackLimits::default()).unwrap();
    let mut after = None;
    let mut seen = Vec::new();
    loop {
        let batch = map.scan(after, 7).unwrap();
        if batch.is_empty() {
            break;
        }
        assert!(batch.len() <= 7);
        for (key, _) in &batch {
            seen.push(key.id);
        }
        after = batch.last().map(|(key, _)| *key);
    }
    assert_eq!(seen, (0..100).collect::<Vec<_>>());
    assert!(map.scan(None, 0).is_err());
    assert!(map.scan(None, 1025).is_err());
    let limits = PackLimits {
        max_scan_bytes: 3,
        ..PackLimits::default()
    };
    let limited = MappedPack::open(dir.path(), &descriptor, limits).unwrap();
    assert_eq!(limited.scan(None, 7).unwrap().len(), 3);
    let empty = compile(dir.path(), vec![record(101, Some(b"")), record(102, None)]);
    let map = MappedPack::open(dir.path(), &empty, PackLimits::default()).unwrap();
    assert!(bytes(map.lookup(key(101)).unwrap()).bytes().is_empty());
    assert!(matches!(
        map.lookup(key(102)).unwrap(),
        PackLookup::Tombstone
    ));
}

#[test]
fn compiler_rejects_unsorted_duplicates_and_limits_without_publishing() {
    for records in [
        vec![record(2, Some(b"x")), record(1, Some(b"y"))],
        vec![record(1, Some(b"x")), record(1, None)],
    ] {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            compile_pack(dir.path(), records, PackLimits::default()),
            Err(PackError::Order)
        ));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    let dir = tempfile::tempdir().unwrap();
    for limits in [
        PackLimits {
            max_record_bytes: 2,
            ..PackLimits::default()
        },
        PackLimits {
            max_file_bytes: 200,
            ..PackLimits::default()
        },
        PackLimits {
            max_directory_bytes: 79,
            ..PackLimits::default()
        },
        PackLimits {
            max_records: 0,
            ..PackLimits::default()
        },
    ] {
        assert!(matches!(
            compile_pack(dir.path(), [record(1, Some(b"abc"))], limits),
            Err(PackError::Limit(_))
        ));
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn payload_and_index_corruption_is_detected_lazily_not_scanned_at_startup() {
    for corrupt_index in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let descriptor = compile(
            dir.path(),
            vec![record(1, Some(b"a")), record(2, Some(b"b"))],
        );
        let path = dir.path().join(&descriptor.file_name);
        let mut raw = std::fs::read(&path).unwrap();
        raw[if corrupt_index { 130 } else { 128 }] ^= 1;
        // Deliberately corrupt BEFORE creating any map. Never mutate mapped files.
        std::fs::write(&path, raw).unwrap();
        let map = MappedPack::open(dir.path(), &descriptor, PackLimits::default()).unwrap();
        assert!(matches!(map.lookup(key(1)), Err(PackError::Integrity)));
        if !corrupt_index {
            assert_eq!(bytes(map.lookup(key(2)).unwrap()).bytes(), b"b");
        }
    }
}

#[test]
fn authenticated_out_of_bounds_record_is_rejected_before_slice_access() {
    let dir = tempfile::tempdir().unwrap();
    let mut descriptor = compile(dir.path(), vec![record(1, Some(b"abc"))]);
    let mut raw = std::fs::read(dir.path().join(&descriptor.file_name)).unwrap();
    let root = u64_at(&raw, 32) as usize;
    let page = u64_at(&raw, root + 32) as usize;
    raw[page + 24..page + 32].copy_from_slice(&u64::MAX.to_le_bytes());
    let page_hash = Sha256::digest(&raw[page..root]);
    raw[root + 48..root + 80].copy_from_slice(&page_hash);
    let mut hasher = Sha256::new();
    hasher.update(&raw[..96]);
    hasher.update(&raw[root..]);
    descriptor.generation = hasher.finalize().into();
    descriptor.file_name = format!("{}.bace", hex(&descriptor.generation));
    raw[96..128].copy_from_slice(&descriptor.generation);
    std::fs::write(dir.path().join(&descriptor.file_name), raw).unwrap();
    let map = MappedPack::open(dir.path(), &descriptor, PackLimits::default()).unwrap();
    assert!(matches!(
        map.lookup(key(1)),
        Err(PackError::Format(_)) | Err(PackError::Limit(_))
    ));
}

#[test]
fn empty_pack_manifest_limits_truncation_and_root_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let descriptor = compile(dir.path(), vec![]);
    let map = MappedPack::open(dir.path(), &descriptor, PackLimits::default()).unwrap();
    assert!(matches!(map.lookup(key(0)).unwrap(), PackLookup::Missing));
    assert!(map.scan(None, 1).unwrap().is_empty());
    drop(map);
    let manifest = PackManifest {
        version: 1,
        generation: 0,
        base: descriptor.clone(),
        deltas: vec![],
    };
    let mut data = manifest.encode(PackLimits::default()).unwrap();
    data[16] ^= 1;
    assert!(matches!(
        PackManifest::decode(&data, PackLimits::default()),
        Err(PackError::Integrity)
    ));
    assert!(
        manifest
            .encode(PackLimits {
                max_segments: 0,
                ..PackLimits::default()
            })
            .is_err()
    );
    data[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        PackManifest::decode(&data, PackLimits::default()),
        Err(PackError::Limit(_))
    ));
    let path = dir.path().join(&descriptor.file_name);
    let mut raw = std::fs::read(&path).unwrap();
    raw[96] ^= 1;
    std::fs::write(&path, &raw).unwrap();
    assert!(matches!(
        MappedPack::open(dir.path(), &descriptor, PackLimits::default()),
        Err(PackError::Integrity)
    ));
    raw.truncate(127);
    std::fs::write(&path, &raw).unwrap();
    assert!(MappedPack::open(dir.path(), &descriptor, PackLimits::default()).is_err());
}

#[test]
fn paged_generation_scan_retains_overrides_tombstones_and_cursor_progress() {
    let dir = tempfile::tempdir().unwrap();
    let limits = PackLimits::default();
    let base = compile(
        dir.path(),
        vec![
            record(1, Some(b"old")),
            record(3, Some(b"third")),
            record(8, Some(b"last")),
        ],
    );
    let delta = compile(
        dir.path(),
        vec![
            record(1, Some(b"new")),
            record(2, Some(b"second")),
            record(3, None),
        ],
    );
    let generation = PackManifest {
        version: 1,
        generation: 2,
        base,
        deltas: vec![delta],
    }
    .open(dir.path(), limits)
    .unwrap();
    let mut cursor = None;
    let mut found = Vec::new();
    loop {
        let page = generation.scan(cursor, 2).unwrap();
        if page.is_empty() {
            break;
        }
        for (k, value) in page {
            assert!(cursor.is_none_or(|old| k > old));
            cursor = Some(k);
            found.push((k, value));
        }
    }
    assert_eq!(
        found.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![key(1), key(2), key(3), key(8)]
    );
    assert_eq!(bytes(found.remove(0).1).bytes(), b"new");
    assert!(matches!(found[1].1, PackLookup::Tombstone));
    assert!(generation.scan(None, 0).is_err());
    assert!(generation.scan(None, 1025).is_err());
}
