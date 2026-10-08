use bace_persistence::{DirtyError, DirtySaves, SaveAck, SaveSnapshot};
use std::time::Duration;
fn snapshot(rev: u64, version: i64) -> SaveSnapshot {
    SaveSnapshot {
        object_id: 7,
        mutation_revision: rev,
        expected_version: version,
        bytes: vec![rev as u8],
    }
}
#[test]
fn coalesces_without_losing_changes_during_a_save() {
    let mut saves = DirtySaves::new(1);
    saves.mark(snapshot(1, 0)).unwrap();
    assert!(saves.due(Duration::from_secs(4)).is_empty());
    assert_eq!(saves.due(Duration::from_secs(5)), vec![snapshot(1, 0)]);
    saves.mark(snapshot(2, 0)).unwrap();
    assert_eq!(saves.reserve(&[7]), Err(DirtyError::Busy));
    saves
        .acknowledge(&SaveAck {
            object_id: 7,
            mutation_revision: 1,
            persisted_version: 1,
        })
        .unwrap();
    assert!(!saves.is_clean());
    assert_eq!(saves.due(Duration::from_secs(10)), vec![snapshot(2, 1)]);
    saves.failed(&snapshot(2, 1)).unwrap();
    assert_eq!(saves.drain_ready(), vec![snapshot(2, 1)]);
}
#[test]
fn bounds_state_and_rejects_stale_acknowledgment() {
    let mut saves = DirtySaves::new(1);
    saves.mark(snapshot(4, 3)).unwrap();
    let mut other = snapshot(1, 0);
    other.object_id = 8;
    assert_eq!(saves.mark(other), Err(DirtyError::Capacity));
    saves.drain_ready();
    assert_eq!(
        saves.acknowledge(&SaveAck {
            object_id: 7,
            mutation_revision: 3,
            persisted_version: 4
        }),
        Err(DirtyError::Stale)
    );
    saves
        .acknowledge(&SaveAck {
            object_id: 7,
            mutation_revision: 4,
            persisted_version: 4,
        })
        .unwrap();
    assert!(saves.is_clean());
    saves.forget_clean(7).unwrap();
}

#[test]
fn valuable_reservations_block_old_saves_and_keep_dirty_state_on_rollback() {
    let mut saves = DirtySaves::new(2);
    saves.mark(snapshot(3, 2)).unwrap();
    assert_eq!(saves.reserve(&[7, 99]), Err(DirtyError::Unknown));
    assert_eq!(saves.reserve(&[7]).unwrap(), vec![snapshot(3, 2)]);
    assert_eq!(saves.mark(snapshot(4, 2)), Err(DirtyError::Busy));
    assert!(saves.drain_ready().is_empty());
    saves.cancel_reserved(&[7]).unwrap();
    assert!(!saves.is_clean());
    saves.reserve(&[7]).unwrap();
    saves.finish_reserved(&[snapshot(4, 3)]).unwrap();
    assert!(saves.is_clean());
    saves.mark(snapshot(5, 3)).unwrap();
    assert_eq!(saves.drain_ready(), vec![snapshot(5, 3)]);
}

#[test]
fn encoded_bytes_are_bounded_and_old_failure_cannot_release_newer_work() {
    let mut saves = DirtySaves::with_byte_limit(10, 4);
    let mut first = snapshot(1, 0);
    first.bytes = vec![1; 4];
    saves.mark(first.clone()).unwrap();
    assert_eq!(saves.retained_bytes(), 4);
    assert_eq!(saves.drain_ready(), vec![first.clone()]);
    assert_eq!(saves.in_flight_bytes(), 4);
    let mut second = snapshot(2, 0);
    second.bytes = vec![2; 3];
    saves.mark(second.clone()).unwrap();
    saves.failed(&first).unwrap();
    assert_eq!(saves.drain_ready(), vec![second.clone()]);
    assert_eq!(saves.failed(&first), Err(DirtyError::Stale));
    assert_eq!(saves.in_flight_bytes(), 3);
    let mut another = snapshot(1, 0);
    another.object_id = 8;
    another.bytes = vec![8; 2];
    assert_eq!(saves.mark(another), Err(DirtyError::Capacity));
    saves.failed(&second).unwrap();
    assert!(!saves.is_clean());
    assert_eq!(saves.drain_ready(), vec![second]);
}
#[test]
fn oversized_committed_state_does_not_bypass_reservation_limit() {
    let mut saves = DirtySaves::with_byte_limit(2, 4);
    saves.mark(snapshot(1, 0)).unwrap();
    saves.reserve(&[7]).unwrap();
    let mut oversized = snapshot(2, 1);
    oversized.bytes = vec![0; 5];
    assert_eq!(
        saves.finish_reserved(&[oversized]),
        Err(DirtyError::Capacity)
    );
    assert_eq!(saves.mark(snapshot(2, 0)), Err(DirtyError::Busy));
    assert_eq!(saves.retained_bytes(), 1);
}

#[test]
fn oldest_unsaved_deadline_survives_hot_coalescing_and_failure() {
    let mut saves = DirtySaves::new(16);
    let mut old = snapshot(1, 0);
    old.object_id = 900;
    saves.mark_at(old.clone(), Duration::ZERO).unwrap();
    let mut hot = snapshot(1, 0);
    hot.object_id = 1;
    saves.mark_at(hot.clone(), Duration::from_secs(1)).unwrap();
    for revision in 2..20 {
        hot.mutation_revision = revision;
        saves.mark_at(hot.clone(), Duration::from_secs(2)).unwrap();
    }
    assert_eq!(saves.dirty_since(1), Some(Duration::from_secs(1)));
    let batch = saves.due(Duration::from_secs(6));
    assert_eq!(
        batch.iter().map(|s| s.object_id).collect::<Vec<_>>(),
        vec![900, 1]
    );
    saves.failed(&old).unwrap();
    assert_eq!(saves.dirty_since(900), Some(Duration::ZERO));
    assert_eq!(saves.due(Duration::from_secs(6))[0].object_id, 900);
}

#[test]
fn successful_hot_snapshot_rotates_age_behind_waiting_dirty_objects() {
    // 1024 is one database batch; the oldest high ID must beat new hot low IDs next turn.
    let mut saves = DirtySaves::new(1100);
    for id in 1..=1025 {
        let mut value = snapshot(1, 0);
        value.object_id = id;
        saves.mark_at(value, Duration::ZERO).unwrap();
    }
    let first = saves.due(Duration::from_secs(5));
    assert_eq!(first.len(), 1024);
    for sent in first {
        let mut newer = sent.clone();
        newer.mutation_revision = 2;
        saves.mark_at(newer, Duration::from_secs(5)).unwrap();
        saves
            .acknowledge(&SaveAck {
                object_id: sent.object_id,
                mutation_revision: 1,
                persisted_version: 1,
            })
            .unwrap();
    }
    assert_eq!(saves.dirty_since(1), Some(Duration::from_secs(5)));
    for id in 1026..=1050 {
        let mut value = snapshot(1, 0);
        value.object_id = id;
        saves.mark_at(value, Duration::from_secs(5)).unwrap();
    }
    let next = saves.due(Duration::from_secs(10));
    assert_eq!(
        next[0].object_id, 1025,
        "old high-ID work cannot be starved by hot low IDs or new arrivals"
    );
}

#[test]
fn younger_small_saves_cannot_keep_old_large_save_waiting_for_byte_capacity() {
    let mut saves = DirtySaves::with_byte_limit(3, 10);
    let mut blocker = snapshot(1, 0);
    blocker.bytes = vec![0; 6];
    saves.mark_at(blocker.clone(), Duration::ZERO).unwrap();
    assert_eq!(saves.drain_ready(), vec![blocker.clone()]);
    let mut shrunk = blocker.clone();
    shrunk.mutation_revision = 2;
    shrunk.bytes = vec![0; 1];
    saves.mark_at(shrunk, Duration::from_secs(1)).unwrap();
    let mut older = snapshot(1, 0);
    older.object_id = 900;
    older.bytes = vec![0; 5];
    saves.mark_at(older, Duration::from_secs(1)).unwrap();
    let mut younger = snapshot(1, 0);
    younger.object_id = 2;
    younger.bytes = vec![0; 4];
    saves.mark_at(younger, Duration::from_secs(2)).unwrap();
    assert!(
        saves.due(Duration::from_secs(7)).is_empty(),
        "do not backfill the four free bytes with younger work"
    );
    saves
        .acknowledge(&SaveAck {
            object_id: 7,
            mutation_revision: 1,
            persisted_version: 1,
        })
        .unwrap();
    let next = saves.due(Duration::from_secs(7));
    assert_eq!(
        next.iter().map(|s| s.object_id).collect::<Vec<_>>(),
        vec![7, 900, 2]
    );
}
#[test]
fn clean_seed_batch_failure_and_hierarchy_handoff_preserve_dirty_age() {
    let mut saves = DirtySaves::with_byte_limit(4, 100);
    saves.register_clean(snapshot(1, 1)).unwrap();
    assert!(saves.is_clean());
    let mut child = snapshot(1, 1);
    child.object_id = 8;
    saves.register_clean(child.clone()).unwrap();
    saves
        .mark_at(snapshot(2, 1), Duration::from_secs(1))
        .unwrap();
    let mut invalid = child.clone();
    invalid.mutation_revision = 0;
    assert!(
        saves
            .mark_batch_at(vec![snapshot(3, 1), invalid], Duration::from_secs(4))
            .is_err()
    );
    let due = saves.due(Duration::from_secs(6));
    assert_eq!(
        due,
        vec![snapshot(2, 1)],
        "failed batch did not partially replace player"
    );
    saves.failed(&due[0]).unwrap();
    saves.reserve(&[7, 8]).unwrap();
    let mut acquired = snapshot(1, 1);
    acquired.object_id = 9;
    saves
        .finish_reserved_hierarchy(&[7, 8], &[snapshot(3, 2)], &[acquired], &[8])
        .unwrap();
    assert!(saves.is_clean());
    assert!(saves.reserve(&[8]).is_err());
    saves.reserve(&[7, 9]).unwrap();
    saves.cancel_reserved(&[7, 9]).unwrap();
}
