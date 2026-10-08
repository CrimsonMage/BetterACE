use bace_gameplay_api::{ActionContext, SessionId, locomotion::*};
use bace_session::*;
use bace_types::{AccountId, EntityId};
fn binding() -> ActionContext {
    ActionContext {
        actor: EntityId(9),
        account: AccountId(8),
        session: SessionId(7),
        sequence: 0,
    }
}
fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn original_csharp_wire_prefixes_bind_actor_and_keep_observations_separate() {
    for line in include_str!("fixtures/locomotion.csv")
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
    {
        let (kind, bytes) = line.split_once(',').unwrap();
        let bytes = hex(bytes);
        let output =
            decode_locomotion(SessionState::WorldConnected, binding(), &bytes, 512).unwrap();
        assert_eq!(output.context.actor, EntityId(9));
        assert_eq!(output.context.sequence, 42);
        assert_eq!(output.epochs.unwrap().teleport, 0x5566);
        assert!(movement_epochs_match(
            output.epochs.unwrap(),
            output.epochs.unwrap()
        ));
        let mut wrong = output.epochs.unwrap();
        wrong.instance = 0;
        assert!(!movement_epochs_match(output.epochs.unwrap(), wrong));
        match kind {
            "jumps" => {
                assert_eq!(output.request, LocomotionRequest::Jump { extent: 0.75 });
                assert!(matches!(
                    output.observations,
                    LocomotionObservations::Jump {
                        object: 0x50000002,
                        spell: 123,
                        ..
                    }
                ));
            }
            "autonomous" => assert_eq!(output.request, LocomotionRequest::ObservePosition),
            "raw" => assert_eq!(
                output.request,
                LocomotionRequest::State(RawLocomotionState {
                    style: 0x8000003d,
                    current_hold: 1,
                    forward: (0x41000003, 0, 1.),
                    sidestep: (0, 0, 1.),
                    turn: (0, 0, 1.)
                })
            ),
            _ => panic!(),
        }
        assert_eq!(
            decode_locomotion(SessionState::AuthConnected, binding(), &bytes, 512),
            Err(DispatchError::WrongState)
        );
        for n in 0..bytes.len() {
            assert!(
                decode_locomotion(SessionState::WorldConnected, binding(), &bytes[..n], 512)
                    .is_err()
            );
        }
        let mut extra = bytes;
        extra.push(0xa5);
        assert!(decode_locomotion(SessionState::WorldConnected, binding(), &extra, 512).is_err());
    }
}
#[test]
fn nonfinite_reported_velocity_or_extent_is_rejected_before_a_command_exists() {
    let line = include_str!("fixtures/locomotion.csv")
        .lines()
        .find(|l| l.starts_with("jumps,"))
        .unwrap();
    let original = hex(line.split_once(',').unwrap().1);
    for offset in [12, 16, 20, 24] {
        let mut bytes = original.clone();
        bytes[offset..offset + 4].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
        assert!(decode_locomotion(SessionState::WorldConnected, binding(), &bytes, 512).is_err());
    }
}
#[test]
fn source_position_jump_and_non_autonomous_extent_route_without_inventing_epochs() {
    for line in include_str!("../../bace-wire/tests/fixtures/physical_jump.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let row: Vec<_> = line.split(',').collect();
        let raw = hex(row[3]);
        let mut packet = 0xf7b1u32.to_le_bytes().to_vec();
        if row[0] == "position" {
            packet.extend(42u32.to_le_bytes());
            packet.extend(0xf61bu32.to_le_bytes());
            packet.extend(raw);
        } else {
            packet.extend(raw);
        }
        let output =
            decode_locomotion(SessionState::WorldConnected, binding(), &packet, 128).unwrap();
        assert_eq!(
            output.request,
            LocomotionRequest::Jump {
                extent: row[2].parse().unwrap()
            }
        );
        assert_eq!(output.context.actor, EntityId(9));
        if row[0] == "position" {
            assert_eq!(output.epochs.unwrap().teleport, 0x5566);
            assert!(
                matches!(output.observations,LocomotionObservations::JumpPosition{position,..} if position.cell==0x12340100)
            );
        } else {
            assert!(output.epochs.is_none());
            assert_eq!(
                output.observations,
                LocomotionObservations::NonAutonomousJump
            );
        }
    }
}
