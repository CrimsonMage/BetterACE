use crate::offline_workspace::{build, editable_record, review};
use bace_content::{LandblockInstanceRowV1, WorldRecordV1};
use bace_storage_codec::{
    PackKey, PackLimits, PackLookup, PackRecord, compile_pack, load_manifest, write_manifest,
};
use sha2::Digest;
use std::fs;
use std::sync::atomic::AtomicBool;

fn instance(landblock: u16) -> WorldRecordV1 {
    WorldRecordV1::LandblockInstance(LandblockInstanceRowV1 {
        guid: 0x800012d9,
        landblock: i32::from(landblock),
        weenie_class_id: 1,
        obj_cell_id: u32::from(landblock) << 16 | 1,
        origin_x: 10.0,
        origin_y: 20.0,
        origin_z: 30.0,
        angles_w: 1.0,
        angles_x: 0.0,
        angles_y: 0.0,
        angles_z: 0.0,
        is_link_child: false,
        last_modified: "2026-10-07 00:00:00".into(),
    })
}

#[test]
fn offline_review_preserves_original_and_updates_both_landblock_indexes() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().join("base");
    let drafts = root.path().join("drafts");
    let output = root.path().join("output");
    fs::create_dir_all(&base).unwrap();
    fs::create_dir_all(drafts.join("world")).unwrap();
    fs::create_dir_all(&output).unwrap();
    let weenie = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n",
    )
    .unwrap();
    let original = instance(0xa260);
    let built = bace_content_tools::build_world_pack(
        &[weenie],
        std::slice::from_ref(&original),
        &base,
        &AtomicBool::new(false),
    )
    .unwrap();
    let initial_bytes = fs::read(&built.file).unwrap();
    let source_file = drafts.join("world/instance.toml");
    fs::write(
        &source_file,
        bace_content_tools::export_world_record(&original).unwrap(),
    )
    .unwrap();
    let unchanged = review(&built.manifest, &drafts).unwrap();
    assert!(unchanged.changes.is_empty());

    let moved = instance(0xa261);
    fs::write(
        &source_file,
        bace_content_tools::export_world_record(&moved).unwrap(),
    )
    .unwrap();
    let reviewed = review(&built.manifest, &drafts).unwrap();
    assert_eq!(reviewed.changes.len(), 3);
    assert_eq!(reviewed.changes[0].action, "Replace");
    let text = editable_record(
        &load_manifest(&built.manifest, PackLimits::default())
            .unwrap()
            .open(&base, PackLimits::default())
            .unwrap(),
        PackKey {
            namespace: 20,
            id: original.id(),
        },
    )
    .unwrap();
    assert_eq!(
        bace_content_tools::parse_world_record(&text).unwrap(),
        original
    );
    let candidate_dir = build(&reviewed, &output, false).unwrap();
    let manifest_path = fs::read_dir(&candidate_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "manifest"))
        .unwrap();
    let candidate = load_manifest(&manifest_path, PackLimits::default())
        .unwrap()
        .open(&candidate_dir, PackLimits::default())
        .unwrap();
    let PackLookup::Record(handle) = candidate
        .lookup(PackKey {
            namespace: 20,
            id: moved.id(),
        })
        .unwrap()
    else {
        panic!("missing moved row")
    };
    assert_eq!(
        bace_content_tools::decode_world_record(handle.bytes()).unwrap(),
        moved
    );
    assert!(matches!(
        candidate
            .lookup(PackKey {
                namespace: 2,
                id: 0xa260
            })
            .unwrap(),
        PackLookup::Tombstone
    ));
    let PackLookup::Record(index) = candidate
        .lookup(PackKey {
            namespace: 2,
            id: 0xa261,
        })
        .unwrap()
    else {
        panic!("missing new landblock index")
    };
    assert_eq!(
        bace_content_tools::decode_landblock_index(index.bytes())
            .unwrap()
            .instance_ids,
        vec![0x800012d9]
    );
    assert_eq!(fs::read(&built.file).unwrap(), initial_bytes);
}

#[test]
fn coordinate_edit_does_not_rewrite_unchanged_landblock_index() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().join("base");
    let drafts = root.path().join("drafts");
    let output = root.path().join("output");
    fs::create_dir_all(&base).unwrap();
    fs::create_dir_all(drafts.join("world")).unwrap();
    fs::create_dir_all(&output).unwrap();
    let weenie = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n",
    )
    .unwrap();
    let original = instance(0xa260);
    let built = bace_content_tools::build_world_pack(
        &[weenie],
        std::slice::from_ref(&original),
        &base,
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut changed = original;
    let WorldRecordV1::LandblockInstance(ref mut row) = changed else {
        panic!("wrong variant")
    };
    row.origin_x = 44.0;
    fs::write(
        drafts.join("world/instance.toml"),
        bace_content_tools::export_world_record(&changed).unwrap(),
    )
    .unwrap();
    let reviewed = review(&built.manifest, &drafts).unwrap();
    assert_eq!(reviewed.changes.len(), 1);
    assert_eq!(reviewed.changes[0].key.namespace, 20);
    let output_dir = build(&reviewed, &output, false).unwrap();
    let manifest_path = fs::read_dir(&output_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "manifest"))
        .unwrap();
    let manifest = load_manifest(&manifest_path, PackLimits::default()).unwrap();
    assert_eq!(manifest.deltas[0].record_count, 1);
}

#[test]
fn full_generation_requires_compaction_confirmation_for_candidate() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().join("base");
    let drafts = root.path().join("drafts");
    let output = root.path().join("output");
    fs::create_dir_all(&base).unwrap();
    fs::create_dir_all(drafts.join("world")).unwrap();
    fs::create_dir_all(&output).unwrap();
    let weenie = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n",
    )
    .unwrap();
    let built = bace_content_tools::build_world_pack(
        &[weenie],
        &[instance(0xa260)],
        &base,
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut manifest = load_manifest(&built.manifest, PackLimits::default()).unwrap();
    for id in 1..=2 {
        manifest.deltas.push(
            compile_pack(
                &base,
                [Ok(PackRecord {
                    key: PackKey { namespace: 99, id },
                    schema: 1,
                    value: Some(vec![id as u8]),
                })],
                PackLimits::default(),
            )
            .unwrap(),
        );
    }
    let full_manifest = write_manifest(&base, &manifest, PackLimits::default()).unwrap();
    fs::write(
        drafts.join("world/instance.toml"),
        bace_content_tools::export_world_record(&instance(0xa261)).unwrap(),
    )
    .unwrap();
    let reviewed = review(&full_manifest, &drafts).unwrap();
    assert!(reviewed.needs_compaction());
    assert!(build(&reviewed, &output, false).is_err());
    let candidate_dir = build(&reviewed, &output, true).unwrap();
    let candidate_manifest_path = fs::read_dir(&candidate_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "manifest"))
        .unwrap();
    let candidate_manifest =
        load_manifest(&candidate_manifest_path, PackLimits::default()).unwrap();
    assert_eq!(candidate_manifest.deltas.len(), 1);
    let candidate = candidate_manifest
        .open(&candidate_dir, PackLimits::default())
        .unwrap();
    assert!(matches!(
        candidate
            .lookup(PackKey {
                namespace: 2,
                id: 0xa260
            })
            .unwrap(),
        PackLookup::Tombstone
    ));
}

#[test]
fn weenie_only_studio_pack_can_receive_a_small_offline_delta() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.toml");
    let base = root.path().join("base");
    let drafts = root.path().join("drafts");
    let output = root.path().join("output");
    fs::create_dir_all(&base).unwrap();
    fs::create_dir_all(drafts.join("weenies")).unwrap();
    fs::create_dir_all(&output).unwrap();
    fs::write(
        &source,
        "schema_version=1\nweenie_id=1\nclass_name='first'\nweenie_type=1\n[properties]\n",
    )
    .unwrap();
    let built =
        bace_content_tools::build_weenie_pack(&[source], &base, &AtomicBool::new(false)).unwrap();
    fs::write(
        drafts.join("weenies/2.toml"),
        "schema_version=1\nweenie_id=2\nclass_name='second'\nweenie_type=1\n[properties]\n",
    )
    .unwrap();
    let reviewed = review(&built.manifest, &drafts).unwrap();
    assert_eq!(reviewed.changes.len(), 1);
    assert_eq!(reviewed.changes[0].action, "Add");
    let candidate_dir = build(&reviewed, &output, false).unwrap();
    let manifest_path = fs::read_dir(&candidate_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "manifest"))
        .unwrap();
    let candidate = load_manifest(&manifest_path, PackLimits::default())
        .unwrap()
        .open(&candidate_dir, PackLimits::default())
        .unwrap();
    assert!(matches!(
        candidate
            .lookup(PackKey {
                namespace: 1,
                id: 2
            })
            .unwrap(),
        PackLookup::Record(_)
    ));
}

#[test]
fn candidate_requires_fresh_review_after_source_changes() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().join("base");
    let drafts = root.path().join("drafts");
    let output = root.path().join("output");
    fs::create_dir_all(&base).unwrap();
    fs::create_dir_all(drafts.join("world")).unwrap();
    fs::create_dir_all(&output).unwrap();
    let weenie = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n",
    )
    .unwrap();
    let built = bace_content_tools::build_world_pack(
        &[weenie],
        &[instance(0xa260)],
        &base,
        &AtomicBool::new(false),
    )
    .unwrap();
    let source_file = drafts.join("world/instance.toml");
    fs::write(
        &source_file,
        bace_content_tools::export_world_record(&instance(0xa261)).unwrap(),
    )
    .unwrap();
    let reviewed = review(&built.manifest, &drafts).unwrap();
    fs::write(
        &source_file,
        bace_content_tools::export_world_record(&instance(0xa262)).unwrap(),
    )
    .unwrap();
    assert!(build(&reviewed, &output, false).is_err());
    assert!(fs::read_dir(&output).unwrap().next().is_none());
}

#[test]
fn emote_trace_applies_supported_text_and_stops_on_unhandled_motion() {
    let mut weenie = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n",
    )
    .unwrap();
    weenie.properties.emotes.push(bace_content::Emote {
        category: 7,
        probability: 1.0,
        actions: vec![
            bace_content::EmoteAction {
                r#type: 8,
                message: Some("hello".into()),
                ..Default::default()
            },
            bace_content::EmoteAction {
                r#type: 5,
                motion: Some(1),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    let trace = crate::emote_sandbox::run(&weenie, 7).unwrap();
    assert!(trace.lines.iter().any(|line| line.contains("hello")));
    assert!(
        trace
            .lines
            .iter()
            .any(|line| line.contains("Unsupported effect"))
    );
    assert!(trace.lines.iter().any(|line| line.contains("Stopped")));
}

#[test]
fn clone_world_row_changes_only_its_primary_identity() {
    let old = instance(0xa260);
    let text = bace_content_tools::export_world_record(&old).unwrap();
    let cloned = crate::offline_workspace::clone_with_id(&text, 20, 0x800012da).unwrap();
    let WorldRecordV1::LandblockInstance(row) =
        bace_content_tools::parse_world_record(&cloned).unwrap()
    else {
        panic!("wrong variant")
    };
    assert_eq!(row.guid, 0x800012da);
    assert_eq!(row.landblock, 0xa260);
    assert_eq!(row.weenie_class_id, 1);
}

#[test]
#[ignore = "Requires BACE_WORLD_MANIFEST pointing to a complete local 16PY pack"]
fn supplied_world_pack_builds_one_quest_delta_without_touching_base() {
    let manifest_path = std::env::var_os("BACE_WORLD_MANIFEST")
        .map(std::path::PathBuf::from)
        .expect("Set BACE_WORLD_MANIFEST to a complete local 16PY pack");
    let limits = PackLimits::default();
    let manifest = load_manifest(&manifest_path, limits).unwrap();
    let source_dir = manifest_path.parent().unwrap();
    let original_pack = source_dir.join(&manifest.base.file_name);
    let original_hash = sha2::Sha256::digest(fs::read(&original_pack).unwrap());
    let generation = manifest.open(source_dir, limits).unwrap();
    let key = generation
        .scan(
            Some(PackKey {
                namespace: 22,
                id: u64::MAX,
            }),
            64,
        )
        .unwrap()
        .into_iter()
        .find_map(|(key, value)| {
            (key.namespace == 23 && matches!(value, PackLookup::Record(_))).then_some(key)
        })
        .expect("The supplied 16PY world has no quest in its first quest page");
    let WorldRecordV1::Quest(mut quest) =
        bace_content_tools::parse_world_record(&editable_record(&generation, key).unwrap())
            .unwrap()
    else {
        panic!("Quest namespace decoded as another row");
    };
    quest.message = Some("Offline Studio candidate validation".into());
    let root = tempfile::tempdir().unwrap();
    let drafts = root.path().join("drafts");
    let output = root.path().join("output");
    fs::create_dir_all(drafts.join("world")).unwrap();
    fs::create_dir_all(&output).unwrap();
    fs::write(
        drafts.join("world/quest.toml"),
        bace_content_tools::export_world_record(&WorldRecordV1::Quest(quest.clone())).unwrap(),
    )
    .unwrap();
    let reviewed = review(&manifest_path, &drafts).unwrap();
    assert_eq!(reviewed.changes.len(), 1);
    assert_eq!(reviewed.changes[0].key, key);
    assert_eq!(reviewed.changes[0].action, "Replace");
    let candidate_dir = build(&reviewed, &output, false).unwrap();
    let candidate_manifest = fs::read_dir(&candidate_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "manifest"))
        .unwrap();
    let candidate = load_manifest(&candidate_manifest, limits)
        .unwrap()
        .open(&candidate_dir, limits)
        .unwrap();
    let WorldRecordV1::Quest(actual) =
        bace_content_tools::parse_world_record(&editable_record(&candidate, key).unwrap()).unwrap()
    else {
        panic!("Candidate quest has wrong kind");
    };
    assert_eq!(actual, quest);
    assert_eq!(
        sha2::Sha256::digest(fs::read(original_pack).unwrap()),
        original_hash
    );
}
