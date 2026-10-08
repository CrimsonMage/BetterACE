use bace_interactions::{DoorAction, DoorAuthority};
#[test]
fn authored_open_is_explicit_and_collision_waits_for_the_generation_hook() {
    let mut door = DoorAuthority::new(false, true, None);
    assert_eq!(door.activate(0, true, true).unwrap(), DoorAction::Locked);
    let DoorAction::Motion(open) = door.scripted_state(true, 0).unwrap() else {
        panic!("authored opening motion");
    };
    assert!(door.locked());
    assert!(!door.physics().ethereal);
    assert_eq!(door.scripted_state(true, 0).unwrap(), DoorAction::Busy);
    door.motion_started(open.generation).unwrap();
    door.ethereal_hook(open.generation, true, true, false)
        .unwrap();
    door.motion_finished(open.generation).unwrap();
    assert!(door.physics().ethereal);
    assert_eq!(door.scripted_state(true, 1).unwrap(), DoorAction::Unchanged);
    let DoorAction::Motion(close) = door.scripted_state(false, 1).unwrap() else {
        panic!("authored closing motion");
    };
    door.motion_started(close.generation).unwrap();
    door.ethereal_hook(close.generation, false, true, true)
        .unwrap();
    assert!(door.physics().ethereal);
    assert!(door.physics().retry_solidity);
    assert!(
        door.ethereal_hook(open.generation, false, true, false)
            .is_err()
    );
    door.retry_solidity(true, false).unwrap();
    assert!(!door.physics().ethereal);
}
