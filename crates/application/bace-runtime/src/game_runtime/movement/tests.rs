use super::*;
use bace_gameplay_api::{
    SessionId,
    locomotion::{LocomotionOutcome, LocomotionRejection},
};
use bace_types::{AccountId, EntityId};
use bace_wire::WirePosition;

#[test]
fn ordinary_retail_prediction_does_not_ratchet_force_position() {
    // GDLE ClientEvents.cpp MoveToState uses >=10m and AutonomousPosition >=25m
    // for a forced correction. A key press can predict ahead of the server tick.
    let view = bace_gameplay_api::visibility::AcceptedObjectView {
        entity: EntityId(1),
        cell: 0x1234_0100,
        position: [0.0, 0.0, 0.0],
        velocity: [0.0; 3],
        heading_radians: 0.0,
        grounded: true,
        epoch: 0,
        held: false,
        motion: Ok(None),
    };
    let observation = |distance| WirePosition {
        cell: view.cell,
        origin: [distance, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    let state = |distance| LocomotionObservations::State {
        position: observation(distance),
        contact: false,
        standing_long_jump: false,
        action_count: 0,
    };
    let autonomous = |distance| LocomotionObservations::Position {
        position: observation(distance),
        contact: false,
    };
    assert!(!needs_forced_position(state(0.5), &view));
    assert!(!needs_forced_position(state(9.9), &view));
    assert!(needs_forced_position(state(10.0), &view));
    assert!(!needs_forced_position(autonomous(24.9), &view));
    assert!(needs_forced_position(autonomous(25.0), &view));
    let mut across_cell = observation(100.0);
    across_cell.cell = 0x1234_0101;
    assert!(!needs_forced_position(
        LocomotionObservations::Position {
            position: across_cell,
            contact: false,
        },
        &view
    ));
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn stale_force_stamp_is_discarded_without_another_forced_position() {
    let mut fixture = Box::pin(crate::game_runtime::tests::entered::fixture()).await;
    let actor = fixture.binding.actor;
    let key = fixture.key;
    let replica = fixture.runtime.players.replication(actor).unwrap();
    let before = replica.properties.current(K::ObjectForcePosition, 0);
    let instance = replica.properties.current(K::ObjectInstance, 0);
    let server_control = replica.properties.current(K::ObjectServerControl, 0);
    let teleport = replica.properties.current(K::ObjectTeleport, 0);
    let canonical = replica
        .properties
        .advance(K::ObjectForcePosition, 0)
        .unwrap();
    assert_eq!(canonical, before.wrapping_add(1));
    let mut payload = bace_wire::Writer::new();
    payload.u32(0x1234_0100);
    for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        payload.f32(value);
    }
    for stamp in [instance, server_control, teleport, before] {
        payload.u16(stamp);
    }
    payload.bytes(&[1, 0, 0, 0]);
    let payload = payload.into_bytes();
    let packet = bace_wire::GameActionEnvelope {
        sequence: 1,
        action: bace_wire::opcode::GameActionType::AutonomousPosition,
        payload: &payload,
    }
    .encode(128)
    .unwrap();
    let message = bace_transport::ReceivedMessage {
        sequence: 100,
        id: 1,
        queue: 3,
        bytes: packet,
    };
    assert!(matches!(
        fixture
            .runtime
            .handle_movement_message(key, &message)
            .unwrap(),
        MovementIngress::Accepted
    ));
    assert!(!fixture.runtime.movement_pending(key));
    for _ in 0..100 {
        fixture.runtime.poll_now().unwrap();
        fixture.client.drain(&fixture.runtime);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        fixture
            .runtime
            .players
            .replication(actor)
            .unwrap()
            .properties
            .current(K::ObjectForcePosition, 0),
        canonical,
        "stale retry advanced ForcePosition"
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn disconnected_motion_receipt_clears_when_outbound_queue_is_full() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let key = SessionKey {
        id: 71,
        generation: 1,
    };
    let context = ActionContext {
        session: SessionId(71),
        account: AccountId(1),
        actor: EntityId(0x5000_0001),
        sequence: 1,
    };
    runtime.movement.pending.insert(
        key,
        Pending {
            context,
            correlation: 1,
            observations: LocomotionObservations::NonAutonomousJump,
            result: Some(Arc::new(LocomotionOutcome {
                correlation: 1,
                context,
                tick: 1,
                result: Err(LocomotionRejection::NotBound),
            })),
        },
    );
    for _ in 0..runtime.limits.messages {
        runtime
            .network_output
            .push_back(NetworkCommand::DrainCompleted { key });
    }
    runtime.poll_movement().unwrap();
    assert!(!runtime.movement_pending(key));
    runtime.network_output.clear();
    runtime.quiesce_now().unwrap();
    for _ in 0..1000 {
        runtime.poll_now().unwrap();
        if !runtime.requires_drain() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(!runtime.requires_drain());
    let mut shutdown = runtime
        .into_shutdown()
        .unwrap_or_else(|(error, _)| panic!("{error}"));
    for _ in 0..1000 {
        if shutdown.poll().unwrap() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(shutdown.complete());
}
