use bace_wire::*;

fn pose() -> WirePosition {
    WirePosition {
        cell: 0x12340001,
        origin: [1.0, 2.0, 3.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}
fn observed_position(raw: bool) -> Vec<u8> {
    let mut writer = Writer::new();
    if raw {
        writer.u32(0);
    }
    pose().write(&mut writer);
    for value in [1, 2, 3, 4] {
        writer.u16(value);
    }
    writer.bytes(&[3]);
    writer.align4();
    writer.into_bytes()
}
#[test]
fn client_movement_lengths_and_command_counts_are_bounded_before_allocation() {
    for raw in [false, true] {
        let bytes = observed_position(raw);
        for length in 0..bytes.len() {
            if raw {
                assert!(ClientMoveToState::decode(&bytes[..length], 128, 4).is_err());
            } else {
                assert!(ClientAutonomousPosition::decode(&bytes[..length], 128).is_err());
            }
        }
        if raw {
            assert!(ClientMoveToState::decode(&bytes, bytes.len() - 1, 4).is_err());
        } else {
            assert!(ClientAutonomousPosition::decode(&bytes, bytes.len() - 1).is_err());
        }
    }
    let mut bytes = observed_position(true);
    for packed in [0x08000000u32, 0xffffu32 << 11, 5u32 << 11] {
        bytes[..4].copy_from_slice(&packed.to_le_bytes());
        assert_eq!(
            ClientMoveToState::decode(&bytes, 128, 4),
            Err(WireError::LimitExceeded)
        );
    }
    bytes[..4].copy_from_slice(&(1u32 << 11).to_le_bytes());
    assert!(ClientMoveToState::decode(&bytes, 128, 4).is_err());
    assert!(ClientJump::decode(&[0; 32], 31).is_err());
    for length in 0..32 {
        assert!(ClientJump::decode(&vec![0; length], 32).is_err());
    }
}
#[test]
fn observation_decoders_do_not_turn_reported_contact_or_floats_into_authority() {
    let mut bytes = observed_position(false);
    bytes[40] = 0xff;
    bytes.extend([0xa5, 0x5a]);
    let observation = ClientAutonomousPosition::decode(&bytes, 128).unwrap();
    assert!(observation.reported_contact);
    assert_eq!(observation.trailing_bytes, 2);
    let mut jump = [0; 32];
    jump[..4].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
    let observation = ClientJump::decode(&jump, 32).unwrap();
    assert!(observation.extent.is_nan()); // downstream physics must reject impossible intent
}
#[test]
fn packed_position_rejects_unknown_flags_and_suffixes() {
    let pack = PositionPack {
        position: pose(),
        velocity: None,
        placement: None,
        grounded: false,
        instance_sequence: 1,
        position_sequence: 2,
        teleport_sequence: 3,
        force_position_sequence: 4,
    };
    let mut bytes = pack.encode();
    bytes[0] |= 0x80;
    assert_eq!(
        PositionPack::decode(&bytes),
        Err(WireError::UnsupportedFlags(0x80))
    );
    let mut bytes = pack.encode();
    bytes.push(0);
    assert_eq!(PositionPack::decode(&bytes), Err(WireError::InvalidLength));
}
#[test]
fn motion_output_refuses_unknown_flags_missing_sticky_fields_and_sequence_aliases() {
    let mut output = MotionUpdate {
        object_id: 1,
        instance_sequence: 2,
        movement_sequence: 3,
        server_control_sequence: 4,
        autonomous: false,
        motion_flags: 1,
        current_style: 0,
        body: MotionBody::State {
            state: InterpretedMotion::default(),
            sticky_object: None,
        },
    };
    assert_eq!(output.encode(1), Err(WireError::InvalidEncoding));
    output.motion_flags = 4;
    assert_eq!(output.encode(1), Err(WireError::UnsupportedFlags(4)));
    output.motion_flags = 0;
    let MotionBody::State { state, .. } = &mut output.body else {
        unreachable!()
    };
    state.commands.push(MotionCommandItem {
        raw_command: 7,
        sequence: 0x8000,
        autonomous: false,
        speed: 1.0,
    });
    assert_eq!(output.encode(0), Err(WireError::LimitExceeded));
    assert_eq!(output.encode(1), Err(WireError::InvalidEncoding));
    let MotionBody::State { state, .. } = &mut output.body else {
        unreachable!()
    };
    state.commands[0].sequence = 0x7fff;
    state.commands[0].autonomous = true;
    let bytes = output.encode(1).unwrap();
    assert_eq!(&bytes[26..28], &[0xff, 0xff]);
}
