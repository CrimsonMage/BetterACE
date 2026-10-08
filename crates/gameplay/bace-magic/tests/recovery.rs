use bace_magic::{CastRecovery, CastRecoveryClock, MagicSchool};
#[test]
fn school_switch_uses_fresh_threshold_but_same_school_is_unrestricted() {
    let mut clock = CastRecoveryClock::default();
    clock
        .record(0.0, 0.0, Some(MagicSchool::War), 10.0)
        .unwrap();
    assert!(clock.school_locked(MagicSchool::Void, 12.999, 0.0).unwrap());
    assert!(!clock.school_locked(MagicSchool::Void, 13.0, 0.0).unwrap());
    assert!(clock.school_locked(MagicSchool::Void, 14.0, 1.0).unwrap());
    assert!(!clock.school_locked(MagicSchool::Void, 15.0, 1.0).unwrap());
    assert!(!clock.school_locked(MagicSchool::War, 10.0, 1.0).unwrap());
    assert!(!clock.school_locked(MagicSchool::Life, 10.0, 1.0).unwrap());
}
#[test]
fn reconnect_rebases_remaining_recovery_without_extending_it() {
    let mut clock = CastRecoveryClock::default();
    clock
        .record(11.0, 12.0, Some(MagicSchool::Void), 10.0)
        .unwrap();
    let snapshot = clock.snapshot(10.5).unwrap();
    let restored = CastRecoveryClock::restore(snapshot, 1.0, 0.25).unwrap();
    assert_eq!(restored.deadlines(), (1.25, 2.25));
    assert!(restored.school_locked(MagicSchool::War, 1.0, 0.0).unwrap());
    assert_eq!(restored.snapshot(1.0).unwrap().last_success_age, 0.75);
    let expired = CastRecoveryClock::restore(snapshot, 1.0, 10.0).unwrap();
    assert_eq!(expired.deadlines(), (1.0, 1.0));
    assert!(!expired.school_locked(MagicSchool::War, 1.0, 1.0).unwrap());
}
#[test]
fn malformed_snapshot_and_revision_overflow_preserve_state() {
    let mut clock = CastRecoveryClock::restore(
        CastRecovery {
            revision: u64::MAX,
            ..Default::default()
        },
        1.0,
        0.0,
    )
    .unwrap();
    let before = clock.snapshot(1.0).unwrap();
    assert!(clock.record(1.0, 1.0, Some(MagicSchool::War), 1.0).is_err());
    assert_eq!(before, clock.snapshot(1.0).unwrap());
    assert!(
        CastRecoveryClock::restore(
            CastRecovery {
                streak_remaining: f64::NAN,
                ..Default::default()
            },
            1.0,
            0.0
        )
        .is_err()
    );
}
#[test]
fn independent_ace_school_recovery_vectors() {
    fn school(value: &str) -> MagicSchool {
        match value {
            "1" => MagicSchool::War,
            "2" => MagicSchool::Life,
            "5" => MagicSchool::Void,
            _ => panic!("oracle school"),
        }
    }
    let mut count = 0;
    for row in include_str!("fixtures/recovery.csv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let fields: Vec<_> = row.split(',').collect();
        let mut clock = CastRecoveryClock::default();
        clock
            .record(0.0, 0.0, Some(school(fields[0])), 10.0)
            .unwrap();
        assert_eq!(
            clock
                .school_locked(
                    school(fields[1]),
                    10.0 + fields[2].parse::<f64>().unwrap(),
                    fields[3].parse().unwrap()
                )
                .unwrap(),
            fields[4] == "1",
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 162);
}
