use super::*;

fn pending() -> (PortalRuntime, PortalDisconnectHandoff) {
    let mut runtime = PortalRuntime::new();
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: bace_types::AccountId(7),
        session: bace_gameplay_api::SessionId(11),
    };
    let view = bace_simulation::PortalAcceptedView {
        actor: binding.actor,
        position: bace_interactions::PortalPosition {
            cell: 0x1234_0001,
            origin: [4., 5., 6.],
            rotation: [1., 0., 0., 0.],
        },
        velocity: [0.; 3],
        grounded: true,
        epoch: 2,
        cloaked: false,
    };
    runtime.transits.insert(
        binding.actor,
        controls::Transit {
            operation: 55,
            view,
            destination_ready: true,
            materialized: false,
        },
    );
    let handoff = PortalDisconnectHandoff {
        key: SessionKey {
            id: 3,
            generation: 11,
        },
        correlation: 91,
        binding,
        operation: 55,
        view,
    };
    (runtime, handoff)
}

#[test]
fn disconnect_waits_for_exact_output_then_retains_transit_until_detach_receipt() {
    let (mut runtime, handoff) = pending();
    let actor = handoff.binding.actor;
    assert!(!runtime.can_handoff(actor, false, false));
    assert!(!runtime.can_handoff(actor, true, true));
    runtime.push(PortalDeliveryWork::Event(PortalServiceEvent::Teleported {
        operation: handoff.operation,
        actors: vec![actor],
        views: vec![handoff.view],
    }));
    assert!(!runtime.can_handoff(actor, true, false));
    runtime.acknowledge(1).unwrap();
    assert!(runtime.can_handoff(actor, true, false));
    runtime.detaching.insert(actor, handoff);
    assert!(runtime.has_pending());
    assert!(!runtime.transits[&actor].materialized);
    assert!(
        runtime
            .complete_disconnect(handoff.key, 92, handoff.binding, true)
            .is_err()
    );
    assert!(runtime.transits.contains_key(&actor));
    assert_eq!(runtime.detaching.get(&actor), Some(&handoff));
    runtime
        .complete_disconnect(handoff.key, 91, handoff.binding, true)
        .unwrap();
    assert!(!runtime.transits.contains_key(&actor));
    assert!(!runtime.has_pending());
    assert!(runtime.controls.is_empty()); // Never fabricates ClientReady.
    assert!(runtime.deliveries.is_empty()); // Never fabricates Materialized.
}

#[test]
fn rejected_detach_and_changed_epoch_never_discard_pending_completion() {
    let (mut runtime, handoff) = pending();
    let actor = handoff.binding.actor;
    runtime.detaching.insert(actor, handoff);
    runtime
        .complete_disconnect(handoff.key, 91, handoff.binding, false)
        .unwrap();
    assert!(runtime.transits.contains_key(&actor));
    assert!(runtime.detaching.is_empty());
    runtime.detaching.insert(actor, handoff);
    runtime.transits.get_mut(&actor).unwrap().view.epoch += 1;
    assert!(
        runtime
            .complete_disconnect(handoff.key, 91, handoff.binding, true)
            .is_err()
    );
    assert!(runtime.transits.contains_key(&actor));
    assert_eq!(runtime.detaching.get(&actor), Some(&handoff));
}

#[test]
fn watched_death_materialization_is_released_only_by_matching_detach_receipt() {
    let (mut runtime, handoff) = pending();
    let actor = handoff.binding.actor;
    runtime.death_materialization_watches.insert(actor, (55, 2));
    runtime.detaching.insert(actor, handoff);
    runtime
        .complete_disconnect(handoff.key, handoff.correlation, handoff.binding, false)
        .unwrap();
    assert!(runtime.materialized_receipts.is_empty());
    runtime.detaching.insert(actor, handoff);
    runtime
        .complete_disconnect(handoff.key, handoff.correlation, handoff.binding, true)
        .unwrap();
    assert_eq!(runtime.materialized_receipts.get(&actor), Some(&(55, 2)));
}
