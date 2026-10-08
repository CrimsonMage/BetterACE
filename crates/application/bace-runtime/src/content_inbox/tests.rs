use super::{scan, write_removal_file};
use bace_content::{QuestRowV1, RecipeRowV1, WorldRecordV1};
use bace_storage_codec::{PackKey, PackLimits, PackRecord};
use std::sync::atomic::AtomicBool;

#[test]
fn inbox_preserves_unchanged_rows_and_requires_explicit_removals() {
    let directory = tempfile::tempdir().unwrap();
    let inbox = tempfile::tempdir().unwrap();
    let weenie = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n",
    )
    .unwrap();
    let quest = WorldRecordV1::Quest(QuestRowV1 {
        id: 2,
        name: "old".into(),
        min_delta: 0,
        max_solves: 1,
        message: None,
        last_modified: "2026-10-08".into(),
    });
    let built = bace_content_tools::build_world_pack(
        std::slice::from_ref(&weenie),
        std::slice::from_ref(&quest),
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest =
        bace_storage_codec::load_manifest(&built.manifest, PackLimits::default()).unwrap();
    let generation = manifest
        .open(directory.path(), PackLimits::default())
        .unwrap();
    let hash = manifest.content_hash(PackLimits::default()).unwrap();
    std::fs::create_dir_all(inbox.path().join("weenies")).unwrap();
    std::fs::write(
        inbox.path().join("weenies/same.toml"),
        bace_content_tools::export(&weenie).unwrap(),
    )
    .unwrap();
    let empty = scan(inbox.path(), hash, &generation).unwrap();
    assert_eq!(empty.preview.total, 0);
    let added = WorldRecordV1::Quest(QuestRowV1 {
        id: 3,
        name: "new".into(),
        min_delta: 0,
        max_solves: 1,
        message: None,
        last_modified: "2026-10-08".into(),
    });
    std::fs::write(
        inbox.path().join("world/new.toml"),
        bace_content_tools::export_world_record(&added).unwrap(),
    )
    .unwrap();
    let recipe = WorldRecordV1::Recipe(RecipeRowV1 {
        id: 4,
        unknown_1: 0,
        skill: 0,
        difficulty: 0,
        salvage_type: 0,
        success_w_c_i_d: 1,
        success_amount: 1,
        success_message: None,
        fail_w_c_i_d: 0,
        fail_amount: 0,
        fail_message: None,
        success_destroy_source_chance: 0.0,
        success_destroy_source_amount: 0,
        success_destroy_source_message: None,
        success_destroy_target_chance: 0.0,
        success_destroy_target_amount: 0,
        success_destroy_target_message: None,
        fail_destroy_source_chance: 0.0,
        fail_destroy_source_amount: 0,
        fail_destroy_source_message: None,
        fail_destroy_target_chance: 0.0,
        fail_destroy_target_amount: 0,
        fail_destroy_target_message: None,
        data_id: 0,
        last_modified: "2026-10-08".into(),
    });
    std::fs::write(
        inbox.path().join("world/recipe.toml"),
        bace_content_tools::export_world_record(&recipe).unwrap(),
    )
    .unwrap();
    std::fs::write(
        inbox.path().join("removals/quest.toml"),
        "kind='quest'\nid=2\n",
    )
    .unwrap();
    let changes = scan(inbox.path(), hash, &generation).unwrap();
    assert_eq!(changes.preview.total, 3);
    assert_eq!(
        changes
            .preview
            .entries
            .iter()
            .map(|e| e.action.as_str())
            .collect::<Vec<_>>(),
        vec!["add", "add", "remove"]
    );
    assert!(changes.weenies.is_empty());
    assert_eq!(changes.mapped.len(), 3);
    assert!(
        changes
            .mapped
            .iter()
            .any(|entry| entry.namespace == 24 && entry.id == 4)
    );
    assert_ne!(changes.preview.token, empty.preview.token);
    let tombstone = bace_storage_codec::compile_pack(
        directory.path(),
        [Ok(PackRecord {
            key: PackKey {
                namespace: 23,
                id: 2,
            },
            schema: 1,
            value: None,
        })],
        PackLimits::default(),
    )
    .unwrap();
    let mut accepted = manifest;
    accepted.generation += 1;
    accepted.deltas.push(tombstone);
    let generation = accepted
        .open(directory.path(), PackLimits::default())
        .unwrap();
    let reviewed_again = scan(
        inbox.path(),
        accepted.content_hash(PackLimits::default()).unwrap(),
        &generation,
    )
    .unwrap();
    assert_eq!(reviewed_again.preview.total, 2);
    assert!(
        reviewed_again
            .preview
            .entries
            .iter()
            .all(|entry| entry.action == "add")
    );
}

#[test]
fn inbox_rejects_duplicate_keys_and_symlinks() {
    let directory = tempfile::tempdir().unwrap();
    let inbox = tempfile::tempdir().unwrap();
    let weenie = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n",
    )
    .unwrap();
    let built = bace_content_tools::build_world_pack(
        &[weenie],
        &[],
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest =
        bace_storage_codec::load_manifest(&built.manifest, PackLimits::default()).unwrap();
    let generation = manifest
        .open(directory.path(), PackLimits::default())
        .unwrap();
    let hash = manifest.content_hash(PackLimits::default()).unwrap();
    std::fs::write(
        inbox.path().join("remove.toml"),
        "[[remove]]\nkind='quest'\nid=99\n",
    )
    .unwrap();
    assert!(scan(inbox.path(), hash, &generation).is_err());
    std::fs::write(
        inbox.path().join("remove.toml"),
        "[[remove]]\nkind='weenie'\nid=1\n[[remove]]\nkind='weenie'\nid=1\n",
    )
    .unwrap();
    assert!(scan(inbox.path(), hash, &generation).is_err());
    #[cfg(unix)]
    {
        std::fs::remove_file(inbox.path().join("remove.toml")).unwrap();
        std::os::unix::fs::symlink(
            directory.path().join("missing"),
            inbox.path().join("weenies/link.toml"),
        )
        .unwrap();
        assert!(scan(inbox.path(), hash, &generation).is_err());
    }
}

#[test]
fn dashboard_removal_staging_is_idempotent() {
    let inbox = tempfile::tempdir().unwrap();
    let first = write_removal_file(inbox.path(), "quest", 7).unwrap();
    let repeated = write_removal_file(inbox.path(), "quest", 7).unwrap();
    assert_eq!(first, repeated);
    assert_eq!(
        std::fs::read_dir(inbox.path().join("removals"))
            .unwrap()
            .count(),
        1
    );
    let text = std::fs::read_to_string(inbox.path().join(first)).unwrap();
    assert!(text.contains("kind = \"quest\""));
    assert!(text.contains("id = 7"));
}
