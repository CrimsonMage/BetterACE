use bace_quests::{QuestProgress, QuestRegistry};
#[test]
fn source_snapshot_preserves_signed_progress_and_rejects_noncanonical_or_duplicate_keys() {
    let progress = QuestProgress {
        last_completed_seconds: 123,
        completions: -1,
    };
    let restored = QuestRegistry::restore_snapshot(91, vec![("FLAG".into(), progress)]).unwrap();
    assert_eq!(restored.revision(), 91);
    assert_eq!(restored.get("flag@comment"), Some(progress));
    assert!(QuestRegistry::restore_snapshot(91, vec![("flag".into(), progress)]).is_err());
    assert!(QuestRegistry::restore_snapshot(91, vec![("FLAG@comment".into(), progress)]).is_err());
    assert!(
        QuestRegistry::restore_snapshot(
            91,
            vec![("FLAG".into(), progress), ("FLAG".into(), progress)]
        )
        .is_err()
    );
    assert!(
        QuestRegistry::restore_snapshot(
            91,
            (0..4097).map(|n| (format!("FLAG{n}"), progress)).collect()
        )
        .is_err()
    );
}
