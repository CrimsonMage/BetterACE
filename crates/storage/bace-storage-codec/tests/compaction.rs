use bace_storage_codec::*;
use std::sync::atomic::AtomicBool;
fn record(id: u64, bytes: Option<&[u8]>) -> Result<PackRecord, PackError> {
    Ok(PackRecord {
        key: PackKey { namespace: 1, id },
        schema: 1,
        value: bytes.map(|b| b.to_vec()),
    })
}
#[test]
fn three_pack_generation_compacts_latest_values_without_invalidating_readers() {
    let dir = tempfile::tempdir().unwrap();
    let limits = PackLimits::default();
    let base = compile_pack(
        dir.path(),
        [
            record(1, Some(b"old")),
            record(2, Some(b"remove")),
            record(4, Some(b"base")),
        ],
        limits,
    )
    .unwrap();
    let a = compile_pack(
        dir.path(),
        [record(1, Some(b"middle")), record(2, None)],
        limits,
    )
    .unwrap();
    let b = compile_pack(
        dir.path(),
        [record(1, Some(b"latest")), record(3, Some(b"new"))],
        limits,
    )
    .unwrap();
    let base_map = MappedPack::open(dir.path(), &base, limits).unwrap();
    let old = match base_map
        .lookup(PackKey {
            namespace: 1,
            id: 1,
        })
        .unwrap()
    {
        PackLookup::Record(h) => h,
        _ => panic!("missing"),
    };
    let manifest = PackManifest {
        version: 1,
        generation: 9,
        base,
        deltas: vec![a, b],
    };
    let generation = manifest.open(dir.path(), limits).unwrap();
    let compact = generation
        .compact(dir.path(), limits, &AtomicBool::new(false))
        .unwrap();
    let map = MappedPack::open(dir.path(), &compact, limits).unwrap();
    let rows = map.scan(None, 100).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.0.id).collect::<Vec<_>>(),
        vec![1, 3, 4]
    );
    let actual: Vec<_> = rows
        .into_iter()
        .map(|(_, r)| match r {
            PackLookup::Record(h) => h.bytes().to_vec(),
            _ => panic!("tombstone leaked"),
        })
        .collect();
    assert_eq!(
        actual,
        vec![b"latest".to_vec(), b"new".to_vec(), b"base".to_vec()]
    );
    assert_eq!(old.bytes(), b"old");
    assert!(
        generation
            .compact(dir.path(), limits, &AtomicBool::new(true))
            .is_err()
    );
    let mut excessive = manifest.clone();
    excessive.deltas.push(compact);
    assert!(
        excessive
            .encode(PackLimits {
                max_segments: 100,
                ..limits
            })
            .is_err()
    );
    assert_eq!(manifest.open(dir.path(), limits).unwrap().revision(), 9);
}
