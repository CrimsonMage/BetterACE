use bace_storage_codec::{MappedPack, PackKey, PackLimits, PackLookup, load_manifest};
use std::sync::atomic::AtomicBool;

#[test]
fn unsorted_native_sources_build_reopenable_mapped_weenie_records() {
    let sources = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for id in [900, 100] {
        let path = sources.path().join(format!("{id}.toml"));
        std::fs::write(
            &path,
            format!(
                "schema_version = 1\nweenie_id = {id}\nclass_name = 'item{id}'\nweenie_type = 1\n"
            ),
        )
        .unwrap();
        paths.push(path);
    }
    let build =
        bace_content_tools::build_weenie_pack(&paths, output.path(), &AtomicBool::new(false))
            .unwrap();
    assert_eq!(build.records, 2);
    let manifest = load_manifest(&build.manifest, PackLimits::default()).unwrap();
    let pack = MappedPack::open(output.path(), &manifest.base, PackLimits::default()).unwrap();
    for id in [100, 900] {
        let PackLookup::Record(record) = pack.lookup(PackKey { namespace: 1, id }).unwrap() else {
            panic!("missing record");
        };
        assert_eq!(record.schema(), 1);
        let template = bace_content_tools::decode(record.bytes()).unwrap();
        assert_eq!(template.weenie_id, id as u32);
        assert_eq!(template.class_name, format!("item{id}"));
    }
    assert!(matches!(
        pack.lookup(PackKey {
            namespace: 1,
            id: 101
        })
        .unwrap(),
        PackLookup::Missing
    ));
}

#[test]
fn duplicate_ids_and_cancellation_never_publish_a_pack() {
    let sources = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let source = sources.path().join("one.toml");
    std::fs::write(
        &source,
        "schema_version=1\nweenie_id=1\nclass_name='one'\nweenie_type=1\n",
    )
    .unwrap();
    assert!(
        bace_content_tools::build_weenie_pack(
            &[source.clone(), source.clone()],
            output.path(),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .contains("Duplicate")
    );
    assert!(
        bace_content_tools::build_weenie_pack(&[source], output.path(), &AtomicBool::new(true))
            .is_err()
    );
    assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 0);
}
