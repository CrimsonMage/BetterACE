use bace_spawning::{SpawnError, SpawnSchedule};
#[test]
fn pressure_retains_due_spawn_and_generation_fences_cancel() {
    let mut schedule = SpawnSchedule::new(2).unwrap();
    schedule.schedule(2, 0, 1, 0, 10).unwrap();
    schedule.schedule(1, 0, 2, 0, 5).unwrap();
    assert_eq!(schedule.schedule(3, 0, 1, 0, 1), Err(SpawnError::Capacity));
    assert_eq!(schedule.schedule(1, 0, 2, 0, 5), Err(SpawnError::Duplicate));
    assert_eq!(schedule.next_due(4), None);
    let first = schedule.next_due(5).unwrap();
    assert_eq!(first.generator, 1);
    assert_eq!(schedule.next_due(100), Some(first));
    schedule.cancel_generator(1, 1);
    assert_eq!(schedule.next_due(100), Some(first));
    assert!(schedule.acknowledge(first));
    assert!(!schedule.acknowledge(first));
    assert_eq!(schedule.next_due(100).unwrap().generator, 2);
}

#[test]
fn held_due_ticket_does_not_hide_later_eligible_work_or_lose_its_deadline() {
    let mut schedule = SpawnSchedule::new(3).unwrap();
    schedule.schedule(1, 0, 10, 0, 5).unwrap();
    schedule.schedule(2, 0, 20, 0, 6).unwrap();
    schedule.schedule(3, 0, 30, 0, 100).unwrap();
    let held = schedule.next_due(50).unwrap();
    let eligible = schedule
        .next_due_matching(50, |ticket| ticket.generator != 1)
        .unwrap();
    assert_eq!(eligible.generator, 2);
    assert!(schedule.acknowledge(eligible));
    assert_eq!(schedule.next_due(50), Some(held));
    assert_eq!(held.due_tick, 5);
    assert!(
        schedule
            .next_due_matching(50, |ticket| ticket.generator != 1)
            .is_none()
    );
    assert!(!schedule.acknowledge(bace_spawning::SpawnTicket {
        generation: 11,
        ..held
    }));
    assert!(schedule.acknowledge(held));
    assert_eq!(schedule.pending(), 1);
}
