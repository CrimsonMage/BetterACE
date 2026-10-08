use bace_interactions::{DoorAction, DoorAuthority, DoorError, DoorPhysics};
fn motion(action: DoorAction) -> u64 {
    let DoorAction::Motion(m) = action else {
        panic!("expected motion")
    };
    assert!(!m.autonomous);
    m.generation
}
#[test]
fn closing_animation_and_collision_solidity_are_separate_and_stale_hooks_cannot_close_reopened_door()
 {
    let mut door = DoorAuthority::new(false, false, Some(30));
    let open = motion(door.activate(0, true, true).unwrap());
    assert!(door.is_closed());
    door.motion_started(open).unwrap();
    assert!(!door.is_closed());
    door.ethereal_hook(open, true, true, false).unwrap();
    door.motion_finished(open).unwrap();
    let close = motion(door.activate(1, true, true).unwrap());
    door.motion_started(close).unwrap();
    assert!(door.is_closed());
    assert_eq!(
        door.ethereal_hook(close, false, true, true).unwrap(),
        DoorPhysics {
            ethereal: true,
            retry_solidity: true
        }
    );
    door.motion_finished(close).unwrap();
    assert!(door.retry_solidity(true, true).unwrap().ethereal);
    let reopen = motion(door.activate(2, true, true).unwrap());
    assert_eq!(
        door.ethereal_hook(close, false, true, false),
        Err(DoorError::StaleAnimation)
    );
    assert!(door.retry_solidity(true, false).unwrap().ethereal);
    door.motion_started(reopen).unwrap();
    door.ethereal_hook(reopen, true, true, false).unwrap();
    door.motion_finished(reopen).unwrap();
    let close = motion(door.activate(3, true, true).unwrap());
    door.motion_started(close).unwrap();
    door.ethereal_hook(close, false, true, true).unwrap();
    assert!(!door.retry_solidity(true, false).unwrap().ethereal);
}
#[test]
fn locks_busy_motion_authoritative_range_and_npc_collision_control_activation() {
    let mut door = DoorAuthority::new(false, true, Some(10));
    assert_eq!(door.activate(0, true, true).unwrap(), DoorAction::Locked);
    assert_eq!(door.activate(0, false, true), Err(DoorError::OutOfRange));
    assert_eq!(
        door.monster_contact(0, false, true).unwrap(),
        DoorAction::Unchanged
    );
    door.set_locked(false);
    assert_eq!(
        door.monster_contact(0, true, true).unwrap(),
        DoorAction::Unchanged
    );
    assert_eq!(
        door.monster_contact(0, false, false).unwrap(),
        DoorAction::Unchanged
    );
    let open = motion(door.monster_contact(0, false, true).unwrap());
    assert_eq!(door.activate(1, true, true).unwrap(), DoorAction::Busy);
    let reset = door.poll_reset(10).unwrap();
    assert!(reset.lock_changed);
    assert_eq!(reset.action, DoorAction::Busy);
    assert!(door.locked());
    door.motion_started(open).unwrap();
    door.motion_finished(open).unwrap();
    let close = motion(door.poll_reset(11).unwrap().action);
    assert_eq!(
        door.ethereal_hook(close, false, false, false),
        Err(DoorError::MissingGeometry)
    );
}
#[test]
fn official_gdle_set_ethereal_vectors() {
    for line in include_str!("fixtures/gdle-solidity.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<u32> = line.split(',').map(|s| s.parse().unwrap()).collect();
        let mut door = DoorAuthority::new(p[0] & 1 != 0, false, None);
        let stamp = motion(door.activate(0, true, true).unwrap());
        let state = door
            .ethereal_hook(stamp, p[1] != 0, true, p[2] != 0)
            .unwrap();
        assert_eq!(
            state,
            DoorPhysics {
                ethereal: p[3] != 0,
                retry_solidity: p[4] != 0
            },
            "{line}"
        );
    }
}
#[test]
fn initially_open_content_restores_its_actual_initial_state() {
    let mut door = DoorAuthority::new(true, false, Some(10));
    let close = motion(door.activate(0, true, true).unwrap());
    door.motion_started(close).unwrap();
    door.motion_finished(close).unwrap();
    assert!(door.is_closed());
    let open = motion(door.poll_reset(10).unwrap().action);
    door.motion_started(open).unwrap();
    assert!(!door.is_closed());
}
