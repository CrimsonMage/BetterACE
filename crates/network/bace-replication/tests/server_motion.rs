use bace_replication::{BatchLimits, SequenceKind as K, Sequences, project_server_motion};
use bace_wire::{InterpretedMotion, MotionBody, MotionCommandItem, MovementDescription};
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 1,
        max_bytes: 512,
        max_message_bytes: 512,
        max_string_bytes: 128,
    }
}
fn view() -> MovementDescription {
    MovementDescription {
        autonomous: false,
        motion_flags: 0,
        current_style: 61,
        body: MotionBody::State {
            sticky_object: None,
            state: InterpretedMotion {
                commands: vec![
                    MotionCommandItem {
                        raw_command: 0x132,
                        sequence: 0,
                        autonomous: false,
                        speed: 2.0
                    };
                    2
                ],
                ..Default::default()
            },
        },
    }
}
#[test]
fn rejected_output_does_not_consume_any_object_or_action_counter() {
    let mut s = Sequences::new(3).unwrap();
    let view = view();
    let mut tiny = limits();
    tiny.max_bytes = 1;
    assert!(project_server_motion(1, &view, &mut s, tiny).is_err());
    for (kind, value) in [
        (K::ObjectMovement, 0),
        (K::ObjectServerControl, 0),
        (K::Motion, 1),
    ] {
        assert_eq!(s.current(kind, 0), value);
    }
    let mut bad = view.clone();
    bad.motion_flags = 4;
    assert!(project_server_motion(1, &bad, &mut s, limits()).is_err());
    let mut bad = view.clone();
    bad.autonomous = true;
    assert!(project_server_motion(1, &bad, &mut s, limits()).is_err());
    if let MotionBody::State { state, .. } = &mut bad.body {
        state.commands[0].speed = f32::NAN;
    }
    bad.autonomous = false;
    assert!(project_server_motion(1, &bad, &mut s, limits()).is_err());
    let output = project_server_motion(1, &view, &mut s, limits()).unwrap();
    assert_eq!(output.queue, 10);
    assert_eq!(s.current(K::ObjectMovement, 0), 1);
    assert_eq!(s.current(K::ObjectServerControl, 0), 1);
    assert_eq!(s.current(K::Motion, 0), 3);
}
#[test]
fn capacity_failure_and_action_wrap_cannot_alias_the_autonomous_bit() {
    let view = view();
    let mut small = Sequences::new(2).unwrap();
    assert!(project_server_motion(1, &view, &mut small, limits()).is_err());
    assert_eq!(small.current(K::ObjectMovement, 0), 0);
    let mut s = Sequences::new(3).unwrap();
    while s.current(K::Motion, 0) != 0x7ffe {
        s.advance(K::Motion, 0).unwrap();
    }
    project_server_motion(1, &view, &mut s, limits()).unwrap();
    assert_eq!(s.current(K::Motion, 0), 0);
}
