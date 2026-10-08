use super::*;
use bace_storage_codec::{PackKey, PackRecord};

#[test]
fn reindex_accepts_only_unchanged_source_records() {
    let directory = tempfile::tempdir().unwrap();
    let build = |rows: Vec<(u16, u64, u8)>| {
        let descriptor = bace_storage_codec::compile_pack(
            directory.path(),
            rows.into_iter().map(|(namespace, id, byte)| {
                Ok(PackRecord {
                    key: PackKey { namespace, id },
                    schema: 1,
                    value: Some(vec![byte]),
                })
            }),
            PackLimits::default(),
        )
        .unwrap();
        MappedPack::open(directory.path(), &descriptor, PackLimits::default()).unwrap()
    };
    let old = build(vec![(1, 1, 10), (2, 1, 20), (20, 1, 30), (48, 1, 50)]);
    let reindexed = build(vec![
        (1, 1, 10),
        (2, 1, 21),
        (3, 1, 40),
        (20, 1, 30),
        (48, 1, 51),
        (49, 1, 60),
    ]);
    verify_reindex(&old, &reindexed, 2).unwrap();
    let changed = build(vec![(1, 1, 11), (2, 1, 20), (20, 1, 30)]);
    assert!(verify_reindex(&old, &changed, 2).is_err());
    let removed = build(vec![(1, 1, 10), (2, 1, 20)]);
    assert!(verify_reindex(&old, &removed, 1).is_err());
    let added = build(vec![(1, 1, 10), (1, 2, 11), (2, 1, 20), (20, 1, 30)]);
    assert!(verify_reindex(&old, &added, 3).is_err());

    // The first upgrade can add the frozen table set. Later reindexes must
    // preserve its accepted bytes, even though it is not counted as a world row.
    let upgraded = build(vec![(1, 1, 10), (20, 1, 30), (52, 1, 70)]);
    verify_reindex(&old, &upgraded, 2).unwrap();
    let replaced = build(vec![(1, 1, 10), (20, 1, 30), (52, 1, 71)]);
    assert!(verify_reindex(&upgraded, &replaced, 2).is_err());
}
