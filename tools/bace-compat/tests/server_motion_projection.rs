//! Projection uses original ACE MovementData/MotionItem serializer goldens,
//! including source counters and all 128 interpreted state flag combinations.
use bace_replication::{BatchLimits, SequenceKind as K, Sequences, project_server_motion};
use bace_wire::{InterpretedMotion, MotionBody, MotionCommandItem, MovementDescription};
use serde_json::Value;
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 1,
        max_bytes: 512,
        max_message_bytes: 512,
        max_string_bytes: 128,
    }
}
fn seeded() -> Sequences {
    let mut s = Sequences::new(8).unwrap();
    for (kind, end) in [
        (K::ObjectInstance, 0x1122),
        (K::ObjectMovement, 0x1233),
        (K::ObjectServerControl, 0x2345),
        (K::Motion, 0x4566),
    ] {
        while s.current(kind, 0) != end {
            s.advance(kind, 0).unwrap();
        }
    }
    s
}
#[test]
fn server_motion_matches_all_original_ace_non_autonomous_state_vectors() {
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/messages.json")).unwrap();
    for row in fixture["vectors"]["movement"]["motions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["autonomous"] == false)
    {
        let flags = row["state_flags"].as_u64().unwrap();
        let command = |name: &str, mask| {
            if flags & mask != 0 {
                Some(row[name].as_u64().unwrap() as u16)
            } else {
                None
            }
        };
        let view = MovementDescription {
            autonomous: false,
            motion_flags: 3,
            current_style: 61,
            body: MotionBody::State {
                sticky_object: Some(0x80000001),
                state: InterpretedMotion {
                    current_style: (flags & 1 != 0).then_some(62),
                    forward_command: command("forward", 2),
                    sidestep_command: command("sidestep", 8),
                    turn_command: command("turn", 32),
                    forward_speed: (flags & 4 != 0).then_some(1.25),
                    sidestep_speed: (flags & 16 != 0).then_some(-2.5),
                    turn_speed: (flags & 64 != 0).then_some(3.75),
                    // Untrusted-looking supplied stamps must be replaced by object owner.
                    commands: vec![MotionCommandItem {
                        raw_command: row["command"].as_u64().unwrap() as u16,
                        sequence: 0xffff,
                        autonomous: true,
                        speed: 1.5,
                    }],
                },
            },
        };
        let mut s = seeded();
        let output = project_server_motion(0x50000001, &view, &mut s, limits()).unwrap();
        let hex = row["message"]["bytes"].as_str().unwrap();
        let expected: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(output.bytes, expected, "state_flags={flags}");
        assert_eq!(output.queue, 10);
        assert_eq!(s.current(K::ObjectMovement, 0), 0x1234);
        assert_eq!(s.current(K::ObjectServerControl, 0), 0x2346);
        assert_eq!(s.current(K::Motion, 0), 0x4567);
    }
}
