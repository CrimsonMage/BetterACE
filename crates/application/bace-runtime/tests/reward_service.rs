use bace_gameplay_api::experience::{ExperienceEvent, ExperienceState};
use bace_runtime::{
    network::NetworkCommand,
    reward_service::{RewardOutput, RewardProjection, RewardService},
};
use bace_session::SessionKey;
use bace_types::EntityId;
fn event() -> ExperienceEvent {
    let before = ExperienceState {
        total: 1,
        available: 1,
        level: 1,
        available_skill_credits: 0,
    };
    ExperienceEvent {
        update_properties: true,
        actor: EntityId(0x50000001),
        before,
        after: ExperienceState {
            total: 2,
            available: 2,
            ..before
        },
        maximum_level: 275,
        quest_amount: None,
        next_credit_level: None,
        vitals: vec![],
    }
}
fn command(id: u16, value: u8) -> NetworkCommand {
    NetworkCommand::SendOrderedBatch {
        key: SessionKey { id, generation: 1 },
        messages: vec![(9, vec![value])],
    }
}
#[test]
fn projection_and_observer_routing_survive_pressure_without_replaying_counters() {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let (_ix, ir) = std::sync::mpsc::sync_channel(1);
    tx.send(event()).unwrap();
    let mut owner = RewardService::new();
    let mut projections = 0;
    let first = owner
        .pump_events(
            &rx,
            &ir,
            4,
            |e| {
                assert!(matches!(e, RewardOutput::Player(_)));
                projections += 1;
                Ok(Some(RewardProjection {
                    actor: EntityId(0x50000001),
                    owner: command(1, 1),
                    observers: vec![],
                }))
            },
            |_| Err("observer backpressure".into()),
            |_| panic!("not routed"),
        )
        .unwrap_err();
    assert_eq!(first, "observer backpressure");
    assert_eq!(projections, 1);
    let second = owner
        .pump_events(
            &rx,
            &ir,
            4,
            |_| panic!("projected counters must not advance twice"),
            |_| Ok(vec![command(2, 2)]),
            Err,
        )
        .unwrap();
    assert!(second.blocked);
    let mut sent = vec![];
    owner
        .pump_events(
            &rx,
            &ir,
            8,
            |_| panic!("no projection replay"),
            |_| panic!("no observer routing replay"),
            |c| {
                let NetworkCommand::SendOrderedBatch { key, .. } = c else {
                    panic!()
                };
                sent.push(key.id);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(sent, vec![1, 2]);
    assert!(!owner.has_pending());
}
#[test]
fn shutdown_recovery_preserves_route_state_and_exact_pending_batch() {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let (_ix, ir) = std::sync::mpsc::sync_channel(1);
    tx.send(event()).unwrap();
    let mut owner = RewardService::new();
    owner
        .pump_events(
            &rx,
            &ir,
            4,
            |_| {
                Ok(Some(RewardProjection {
                    actor: EntityId(0x50000001),
                    owner: command(1, 9),
                    observers: vec![],
                }))
            },
            |_| Ok(vec![]),
            Err,
        )
        .unwrap();
    let recovered = owner.recover();
    assert!(recovered.routed);
    assert!(recovered.event.is_some());
    assert!(recovered.projection.is_none());
    assert_eq!(recovered.commands.len(), 1);
}
