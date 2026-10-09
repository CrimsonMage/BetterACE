use super::*;
use bace_gameplay_api::visibility::{
    AcceptedMotionAction, AcceptedMotionDomain, AcceptedObjectMotion, AcceptedObjectView,
};
use bace_replication::SequenceKind;

#[tokio::test]
async fn death_start_uses_exact_motion_bytes_before_advancing_private_or_observer_owners() {
    // Pinned ACE GameMessageUpdateMotion is queue 10 and shares its object
    // counter across private and observer publication. The accepted death
    // action is frozen; this test applies tight retained-output pressure.
    let (_cluster, _directory, mut runtime, _key, binding) =
        crate::game_runtime::portals::tests::output_runtime().await;
    let actor = binding.actor;
    let accepted = AcceptedObjectView {
        entity: actor,
        cell: 0x1234_0001,
        position: [1., 2., 3.],
        velocity: [0.; 3],
        heading_radians: 0.,
        grounded: true,
        epoch: 1,
        held: false,
        motion: Ok(Some(AcceptedObjectMotion {
            autonomous: false,
            style: 0x3d,
            forward_motion: 0,
            forward_rate: 0.,
            sidestep_rate: 0.,
            turn_rate: 0.,
            actions: vec![AcceptedMotionAction {
                domain: AcceptedMotionDomain::Death,
                owner: 7,
                sequence: 1,
                motion: 0x4000_0011, // ACE MotionCommand.Dead
                speed: 1.,
            }],
            goal: None,
        })),
    };
    let motion = bace_replication::project_accepted_server_motion(&accepted).unwrap();
    let (expected, before_event, before_movement) = {
        let replica = runtime.players.replication(actor).unwrap();
        let mut counters = replica.properties.proposal_copy();
        let packet = bace_replication::project_server_motion(
            actor.0,
            &motion,
            &mut counters,
            BatchLimits {
                max_messages: 1,
                max_bytes: runtime.limits.message_bytes,
                max_message_bytes: runtime.limits.message_bytes,
                max_string_bytes: 4096,
            },
        )
        .unwrap();
        (
            packet,
            replica.events.next_sequence(),
            replica.properties.current(SequenceKind::ObjectMovement, 0),
        )
    };
    assert!(expected.bytes.len() < 8192);
    assert!(!runtime.visibility.service.pending());
    runtime
        .deaths
        .push(DeathDeliveryWork::Event(PlayerDeathEvent::Started {
            operation: 7,
            actor,
            motion: 0x4000_0011,
            until_tick: 30,
            num_deaths: 1,
            death_level: None,
            vitae_pool: None,
            vitae: None,
            purge_bad: false,
            suicide: false,
            accepted: Ok(accepted),
        }));
    let budget = 16 * 1024 * 1024;
    let hold = |runtime: &mut GameRuntime, remaining: usize| {
        runtime
            .retain_observer_messages(vec![(
                actor,
                vec![ReplicationMessage {
                    queue: 10,
                    bytes: vec![0; budget - remaining],
                }],
            )])
            .unwrap();
    };
    hold(&mut runtime, expected.bytes.len() - 1);
    runtime.project_death_deliveries().unwrap();
    assert!(runtime.pending_death_delivery().is_some());
    assert!(runtime.network_output.is_empty());
    {
        let replica = runtime.players.replication(actor).unwrap();
        assert_eq!(replica.events.next_sequence(), before_event);
        assert_eq!(
            replica.properties.current(SequenceKind::ObjectMovement, 0),
            before_movement
        );
    }
    let old = runtime.pending_reward_observers().unwrap().sequence;
    runtime.acknowledge_reward_observers(old).unwrap();

    hold(&mut runtime, expected.bytes.len());
    assert!(!runtime.observer_room(1, 8192));
    assert!(runtime.observer_room(1, expected.bytes.len()));
    runtime.project_death_deliveries().unwrap();
    assert!(runtime.pending_death_delivery().is_none());
    assert_eq!(runtime.network_output.len(), 1);
    let filler = runtime.pending_reward_observers().unwrap().sequence;
    runtime.acknowledge_reward_observers(filler).unwrap();
    let observer = runtime.pending_reward_observers().unwrap();
    assert_eq!(observer.messages.len(), 1);
    assert_eq!(observer.messages[0].queue, 10);
    assert_eq!(observer.messages[0].bytes, expected.bytes);
    let replica = runtime.players.replication(actor).unwrap();
    assert!(replica.events.next_sequence() > before_event);
    assert_ne!(
        replica.properties.current(SequenceKind::ObjectMovement, 0),
        before_movement
    );
}
