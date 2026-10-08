//! Ordering from pinned ACE Player_Location.Teleport/OnTeleportComplete;
//! constituent serializer byte parity lives in bace-compat message_* goldens.
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::{
    BatchLimits, PortalPhase, PortalView, SequenceKind as K, Sequences, project_portal,
};
use bace_types::{AccountId, EntityId};
use bace_wire::{PositionPack, PositionUpdate, WirePosition};
fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(0x50000001),
    }
}
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 3,
        max_bytes: 512,
        max_message_bytes: 256,
        max_string_bytes: 64,
    }
}
fn view() -> PortalView {
    PortalView {
        object: binding().actor.0,
        position: PositionPack {
            position: WirePosition {
                cell: 0xa260000a,
                origin: [24.0, 48.0, 18.0],
                rotation: [1.0, 0.0, 0.0, 0.0],
            },
            velocity: Some([0.0; 3]),
            placement: None,
            grounded: true,
            instance_sequence: 999,
            position_sequence: 999,
            teleport_sequence: 999,
            force_position_sequence: 999,
        },
        physics_state: Some(0x4010),
    }
}
fn opcode(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}
#[test]
fn portal_owner_loads_before_shared_position_and_materialization_uses_same_counter_owner() {
    let mut sequences = Sequences::new(5).unwrap();
    sequences.advance(K::ObjectInstance, 0).unwrap();
    sequences.advance(K::ObjectForcePosition, 0).unwrap();
    let hide = project_portal(
        binding(),
        PortalPhase::Hide,
        view(),
        &mut sequences,
        limits(),
    )
    .unwrap();
    assert_eq!(hide.owner.messages, hide.observers);
    assert!(!hide.reset_visibility);
    assert_eq!(sequences.current(K::ObjectTeleport, 0), 0);
    for expected in 1..=2 {
        // The second identical destination is a same-cell recall: stale removal
        // work still needs invalidation by the visibility owner.
        let output = project_portal(
            binding(),
            PortalPhase::Teleport,
            view(),
            &mut sequences,
            limits(),
        )
        .unwrap();
        assert!(output.reset_visibility);
        assert!(output.owner.messages.iter().all(|m| m.queue == 10));
        assert_eq!(
            output
                .owner
                .messages
                .iter()
                .map(|m| opcode(&m.bytes))
                .collect::<Vec<_>>(),
            [0xf751, 0xf748, 0xf74b]
        );
        assert_eq!(output.observers, output.owner.messages[1..]);
        let teleport = &output.owner.messages[0].bytes;
        assert_eq!(&teleport[4..], &[expected as u8, 0, 0, 0]); // source u16 + two padding bytes
        let position = PositionUpdate::decode(&output.owner.messages[1].bytes).unwrap();
        assert_eq!(position.pack.instance_sequence, 1);
        assert_eq!(position.pack.force_position_sequence, 1);
        assert_eq!(position.pack.teleport_sequence, expected);
        assert_eq!(position.pack.position_sequence, expected);
        assert_eq!(position.pack.velocity, None);
    }
    let mut materialized = view();
    materialized.physics_state = Some(8);
    let output = project_portal(
        binding(),
        PortalPhase::Materialize,
        materialized,
        &mut sequences,
        limits(),
    )
    .unwrap();
    assert_eq!(output.observers, output.owner.messages);
    assert_eq!(output.owner.messages.len(), 1);
    assert_eq!(sequences.current(K::ObjectState, 0), 3);
    assert_eq!(sequences.current(K::ObjectTeleport, 0), 2);
}
#[test]
fn capacity_and_invalid_views_preserve_all_sequences() {
    for capacity in [2, 3] {
        let mut sequences = Sequences::new(capacity).unwrap();
        let mut tiny = limits();
        tiny.max_bytes = 9;
        assert!(
            project_portal(
                binding(),
                PortalPhase::Teleport,
                view(),
                &mut sequences,
                tiny
            )
            .is_err()
        );
        let mut invalid = view();
        invalid.position.position.rotation = [0.0; 4];
        assert!(
            project_portal(
                binding(),
                PortalPhase::Teleport,
                invalid,
                &mut sequences,
                limits()
            )
            .is_err()
        );
        if capacity == 2 {
            assert!(
                project_portal(
                    binding(),
                    PortalPhase::Teleport,
                    view(),
                    &mut sequences,
                    limits()
                )
                .is_err()
            );
        }
        for kind in [K::ObjectTeleport, K::ObjectPosition, K::ObjectState] {
            assert_eq!(sequences.current(kind, 0), 0);
        }
    }
}
