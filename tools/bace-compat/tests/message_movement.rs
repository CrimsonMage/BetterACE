//! Movement layout goldens from pinned ACE serializers and action/structure decoders.
//! These establish wire layout, never acceptance of a client pose or jump velocity.
use bace_wire::*;
use serde_json::Value;
fn fixtures() -> Value {
    serde_json::from_str(include_str!("../fixtures/messages.json")).unwrap()
}
fn hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}
fn bytes(value: &Value) -> Vec<u8> {
    hex(value["bytes"].as_str().unwrap())
}
fn vector<const N: usize>(value: &Value) -> [f32; N] {
    std::array::from_fn(|i| value[i].as_f64().unwrap() as f32)
}
fn pose(value: &Value) -> WirePosition {
    WirePosition {
        cell: value["cell"].as_u64().unwrap() as u32,
        origin: vector(&value["origin"]),
        rotation: vector(&value["rotation"]),
    }
}
fn epochs() -> MovementEpochs {
    MovementEpochs {
        instance: 0x1122,
        server_control: 0x3344,
        teleport: 0x5566,
        force_position: 0x7788,
    }
}
fn check_message(actual: &[u8], expected: &Value, group: u32) {
    assert_eq!(actual, bytes(expected));
    assert_eq!(expected["group"].as_u64().unwrap(), u64::from(group));
}

#[test]
fn all_128_official_position_flag_combinations_preserve_omissions_and_counters() {
    let fixtures = fixtures();
    let entries = fixtures["vectors"]["movement"]["positions"]
        .as_array()
        .unwrap();
    assert_eq!(entries.len(), 128);
    for entry in entries {
        let flags = entry["flags"].as_u64().unwrap() as u32;
        let pack = PositionPack {
            position: WirePosition {
                cell: 0x12340001,
                origin: [1.25, -2.5, 3.75],
                rotation: std::array::from_fn(|i| {
                    if flags & (8 << i) != 0 {
                        0.0
                    } else {
                        (i + 1) as f32
                    }
                }),
            },
            velocity: (flags & 1 != 0).then_some([5.0, 6.0, 7.0]),
            placement: (flags & 2 != 0).then_some(8),
            grounded: flags & 4 != 0,
            instance_sequence: 0x1122,
            position_sequence: 0x3344,
            teleport_sequence: 0x5566,
            force_position_sequence: 0x7788,
        };
        let expected = bytes(entry);
        assert_eq!(pack.encode(), expected, "flags {flags:#x}");
        assert_eq!(PositionPack::decode(&expected).unwrap(), pack);
        for length in 0..expected.len() {
            assert!(PositionPack::decode(&expected[..length]).is_err());
        }
    }
}

#[test]
fn official_position_vector_and_autonomous_output_have_distinct_layouts() {
    let fixtures = fixtures();
    let outputs = &fixtures["vectors"]["movement"]["outputs"];
    let mut update = PositionUpdate {
        object_id: 0x50000001,
        pack: PositionPack {
            position: WirePosition {
                cell: 0x12340001,
                origin: [1.25, -2.5, 3.75],
                rotation: [0.1, 0.2, 0.3, 0.4],
            },
            velocity: Some([5.0, 6.0, 7.0]),
            placement: Some(8),
            grounded: true,
            instance_sequence: 0x1122,
            position_sequence: 0x3344,
            teleport_sequence: 0x5566,
            force_position_sequence: 0x7788,
        },
    };
    check_message(&update.encode(), &outputs["position"], 10);
    assert_eq!(
        PositionUpdate::decode(&bytes(&outputs["position"])).unwrap(),
        update
    );
    update.pack.teleport_sequence = 0x5567;
    check_message(&update.encode(), &outputs["admin_position"], 10);
    check_message(
        &VectorUpdate {
            object_id: 0x50000001,
            velocity: [5.0, 6.0, 7.0],
            omega: [8.0, 9.0, 10.0],
            instance_sequence: 0x1122,
            vector_sequence: 0x3456,
        }
        .encode(),
        &outputs["vector"],
        10,
    );
    check_message(
        &AutonomousPositionOutput {
            object_id: 0x50000001,
            origin: [1.25, -2.5, 3.75],
            rotation: [0.1, 0.2, 0.3, 0.4],
            epochs: MovementEpochs {
                server_control: 0x2345,
                ..epochs()
            },
        }
        .encode(),
        &outputs["autonomous"],
        7,
    );
}

#[test]
fn jump_and_autonomous_observations_match_official_handlers_including_ignored_suffix() {
    let fixtures = fixtures();
    let movement = &fixtures["vectors"]["movement"];
    for entry in movement["jumps"].as_array().unwrap() {
        let source = bytes(entry);
        let envelope = GameActionEnvelope::decode(&source, 128).unwrap();
        let jump = ClientJump::decode(envelope.payload, 128).unwrap();
        assert_eq!(jump.extent, entry["extent"].as_f64().unwrap() as f32);
        assert_eq!(jump.reported_velocity, vector(&entry["velocity"]));
        assert_eq!(jump.epochs, epochs());
        assert_eq!(jump.object_id, 0x50000002);
        assert_eq!(jump.spell_id, 123);
        assert_eq!(
            jump.trailing_bytes as u64,
            entry["trailing_bytes"].as_u64().unwrap()
        );
        assert_eq!(
            (source.len() - jump.trailing_bytes) as u64,
            entry["consumed"].as_u64().unwrap()
        );
        for length in 0..32 {
            assert!(ClientJump::decode(&envelope.payload[..length], 128).is_err());
        }
    }
    for entry in movement["autonomous"].as_array().unwrap() {
        let source = bytes(entry);
        let envelope = GameActionEnvelope::decode(&source, 128).unwrap();
        let observed = ClientAutonomousPosition::decode(envelope.payload, 128).unwrap();
        assert_eq!(observed.reported_position, pose(&entry["pose"]));
        assert_eq!(observed.epochs, epochs());
        assert_eq!(
            observed.reported_contact,
            entry["contact"].as_bool().unwrap()
        );
        assert_eq!(
            observed.trailing_bytes as u64,
            entry["trailing_bytes"].as_u64().unwrap()
        );
        assert_eq!(
            (source.len() - observed.trailing_bytes) as u64,
            entry["consumed"].as_u64().unwrap()
        );
    }
}

#[test]
fn raw_motion_conditional_fields_commands_and_contact_match_official_struct_decoder() {
    let fixtures = fixtures();
    for entry in fixtures["vectors"]["movement"]["raw"].as_array().unwrap() {
        let source = bytes(entry);
        let envelope = GameActionEnvelope::decode(&source, 512).unwrap();
        let state = ClientMoveToState::decode(envelope.payload, 512, 2).unwrap();
        assert_eq!(state.reported_position, pose(&entry["pose"]));
        assert_eq!(state.epochs, epochs());
        assert_eq!(
            state.reported_contact(),
            entry["contact"].as_bool().unwrap()
        );
        assert_eq!(
            state.reported_standing_long_jump(),
            entry["standing_long_jump"].as_bool().unwrap()
        );
        assert_eq!(
            state.trailing_bytes as u64,
            entry["trailing_bytes"].as_u64().unwrap()
        );
        assert_eq!(
            (source.len() - state.trailing_bytes) as u64,
            entry["consumed"].as_u64().unwrap()
        );
        let flags = entry["flags"].as_u64().unwrap();
        let motion = &state.motion;
        let words = [
            motion.current_hold_key,
            motion.current_style,
            motion.forward_command,
            motion.forward_hold_key,
            motion.sidestep_command,
            motion.sidestep_hold_key,
            motion.turn_command,
            motion.turn_hold_key,
        ];
        for (index, (actual, mask)) in words
            .into_iter()
            .zip([1, 2, 4, 8, 32, 64, 256, 512])
            .enumerate()
        {
            let expected =
                (flags & mask != 0).then(|| entry["words"][index].as_u64().unwrap() as u32);
            assert_eq!(actual, expected);
        }
        for (index, (actual, mask)) in [
            motion.forward_speed,
            motion.sidestep_speed,
            motion.turn_speed,
        ]
        .into_iter()
        .zip([16, 128, 1024])
        .enumerate()
        {
            let expected =
                (flags & mask != 0).then(|| entry["speeds"][index].as_f64().unwrap() as f32);
            assert_eq!(actual, expected);
        }
        if let Some(commands) = entry["commands"].as_array() {
            assert_eq!(motion.commands.len(), commands.len());
            for (actual, expected) in motion.commands.iter().zip(commands) {
                assert_eq!(
                    u64::from(actual.raw_command),
                    expected["raw_command"].as_u64().unwrap()
                );
                assert_eq!(
                    u64::from(actual.sequence),
                    expected["sequence"].as_u64().unwrap()
                );
                assert_eq!(actual.autonomous, expected["autonomous"].as_bool().unwrap());
                assert_eq!(actual.speed, expected["speed"].as_f64().unwrap() as f32);
            }
        } else {
            assert!(motion.commands.is_empty());
        }
    }
}

fn update(body: MotionBody, autonomous: bool, flags: u8, style: u16) -> MotionUpdate {
    MotionUpdate {
        object_id: 0x50000001,
        instance_sequence: 0x1122,
        movement_sequence: 0x1234,
        server_control_sequence: if autonomous { 0x2345 } else { 0x2346 },
        autonomous,
        motion_flags: flags,
        current_style: style,
        body,
    }
}
#[test]
fn all_interpreted_motion_flags_and_autonomy_counters_match_official_output() {
    let fixtures = fixtures();
    let movement = &fixtures["vectors"]["movement"];
    for entry in movement["motions"].as_array().unwrap() {
        let flags = entry["state_flags"].as_u64().unwrap();
        let autonomous = entry["autonomous"].as_bool().unwrap();
        let word =
            |name: &str, mask| (flags & mask != 0).then(|| entry[name].as_u64().unwrap() as u16);
        let state = InterpretedMotion {
            current_style: (flags & 1 != 0).then_some(62),
            forward_command: word("forward", 2),
            sidestep_command: word("sidestep", 8),
            turn_command: word("turn", 32),
            forward_speed: (flags & 4 != 0).then_some(1.25),
            sidestep_speed: (flags & 16 != 0).then_some(-2.5),
            turn_speed: (flags & 64 != 0).then_some(3.75),
            commands: vec![MotionCommandItem {
                raw_command: entry["command"].as_u64().unwrap() as u16,
                sequence: 0x4567,
                autonomous,
                speed: 1.5,
            }],
        };
        check_message(
            &update(
                MotionBody::State {
                    state,
                    sticky_object: Some(0x80000001),
                },
                autonomous,
                3,
                61,
            )
            .encode(1)
            .unwrap(),
            &entry["message"],
            10,
        );
    }
    check_message(
        &update(
            MotionBody::State {
                state: InterpretedMotion::default(),
                sticky_object: None,
            },
            true,
            0,
            0,
        )
        .encode(0)
        .unwrap(),
        &movement["outputs"]["empty_motion"],
        10,
    );
}

#[test]
fn all_four_directed_motion_branches_match_official_serializer() {
    let fixtures = fixtures();
    let parameters = MoveToParameters {
        flags: 0x1efff,
        distance_to_object: 0.6,
        min_distance: 0.1,
        fail_distance: 100.0,
        speed: 1.5,
        walk_run_threshold: 15.0,
        desired_heading: 90.0,
    };
    let turn = TurnToParameters {
        flags: 0x1efff,
        speed: 1.5,
        desired_heading: 180.0,
    };
    for entry in fixtures["vectors"]["movement"]["directed"]
        .as_array()
        .unwrap()
    {
        let body = match entry["type"].as_u64().unwrap() {
            6 => MotionBody::MoveToObject {
                target: 0x80000001,
                cell: 0x12340001,
                origin: [1.25, -2.5, 3.75],
                parameters,
                run_rate: 2.5,
            },
            7 => MotionBody::MoveToPosition {
                cell: 0x12340001,
                origin: [1.25, -2.5, 3.75],
                parameters,
                run_rate: 2.5,
            },
            8 => MotionBody::TurnToObject {
                target: 0x80000001,
                desired_heading: 180.0,
                parameters: turn,
            },
            9 => MotionBody::TurnToHeading { parameters: turn },
            _ => panic!("unexpected fixture branch"),
        };
        check_message(
            &update(body, false, 0, 61).encode(0).unwrap(),
            &entry["message"],
            10,
        );
    }
}
