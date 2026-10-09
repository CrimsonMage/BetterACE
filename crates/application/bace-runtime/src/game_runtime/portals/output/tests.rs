use super::*;
#[test]
fn original_ace_portal_flags_and_conditional_broadcasts() {
    let mut count = 0;
    for line in include_str!("../../../../tests/fixtures/portal_state.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<u32> = line.split(',').map(|v| v.parse().unwrap()).collect();
        let phase = if row[2] == 0 {
            PortalPhase::Teleport
        } else {
            PortalPhase::Materialize
        };
        let after = portal_state(row[0], phase, row[1] == 2);
        assert_eq!(after, row[3], "{line}");
        let broadcast = u32::from(phase == PortalPhase::Materialize || after != row[0]);
        assert_eq!(broadcast, row[4], "{line}");
        assert_eq!(portal_state(row[0], PortalPhase::Hide, row[1] == 2), row[0]);
        count += 1;
    }
    assert_eq!(count, 80);
}
#[test]
fn output_view_order_and_capacity_are_checked_before_canonical_projection() {
    let view = bace_simulation::PortalAcceptedView {
        actor: EntityId(1),
        position: bace_interactions::PortalPosition {
            cell: 1,
            origin: [0.; 3],
            rotation: [1., 0., 0., 0.],
        },
        velocity: [0.; 3],
        grounded: true,
        epoch: 1,
        cloaked: false,
    };
    assert!(validate_views(&[EntityId(1)], &[view]).is_ok());
    assert!(validate_views(&[EntityId(2)], &[view]).is_err());
    assert!(validate_views(&[EntityId(1)], &[]).is_err());
    assert!(validate_views(&[], &[]).is_err());
    assert!(validate_views(&[EntityId(1); 10], &[view; 10]).is_err());
}

fn link_ticket(operation: u64, actor: EntityId, slot: u16) -> bace_simulation::PortalServiceTicket {
    let position = bace_interactions::PortalPosition {
        cell: 0x1234_0001,
        origin: [4., 5., 6.],
        rotation: [1., 0., 0., 0.],
    };
    bace_simulation::PortalServiceTicket {
        origin: bace_simulation::PortalServiceOrigin::Spell,
        operation,
        cast: operation + 100,
        actor,
        cast_actor: actor,
        before_revision: 1,
        after_revision: 2,
        participants: vec![(actor, 1, 2)],
        mana: None,
        effect: bace_simulation::PortalServiceEffect::Link(bace_interactions::PortalLinkMutation {
            before_revision: 1,
            after_revision: 2,
            position_slot: slot,
            before: None,
            after: position,
            data_id: match slot {
                8 => Some((31, None, 0x1234)),
                16 => Some((48, None, 0x1234)),
                _ => None,
            },
            tied_summoned: false,
        }),
    }
}

fn stage_link(
    runtime: &mut GameRuntime,
    binding: bace_gameplay_api::CharacterBinding,
    ticket: bace_simulation::PortalServiceTicket,
) {
    let operation = ticket.operation;
    runtime
        .portals
        .ticket_bindings
        .insert(operation, vec![binding]);
    runtime.portals.tickets.insert(operation, ticket.clone());
    runtime
        .portals
        .push(PortalDeliveryWork::Event(PortalServiceEvent::Linked {
            operation,
            actor: binding.actor,
        }));
    runtime
        .portals
        .push(PortalDeliveryWork::Completed(Box::new(PortalCompletion {
            work: PortalWork {
                epoch: 1,
                bindings: vec![binding],
                ticket,
            },
            committed: true,
            aborted: false,
        })));
}

#[tokio::test]
async fn source_spell_link_slots_project_exact_lifestone_and_portal_chat() {
    // ACE WorldObject_Magic.HandleCastSpell_PortalLink sends the life-stone
    // line for LifestoneTie1; the simulation stores it in slot 15. PortalTie1
    // and PortalTie2 use slots 8 and 16 and share the portal line.
    let (_cluster, _directory, mut runtime, key, binding) =
        crate::game_runtime::portals::tests::output_runtime().await;
    for (operation, slot, text) in [
        (151, 15, "You have successfully linked with the life stone."),
        (152, 8, "You have successfully linked with the portal."),
        (153, 16, "You have successfully linked with the portal."),
    ] {
        stage_link(
            &mut runtime,
            binding,
            link_ticket(operation, binding.actor, slot),
        );
        runtime.project_portal_deliveries().unwrap();
        let NetworkCommand::SendOrderedBatch {
            key: recipient,
            messages,
        } = runtime.network_output.pop_front().unwrap()
        else {
            panic!("exact portal-link private output");
        };
        assert_eq!(recipient, key);
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].1,
            bace_wire::ChatMessage::System { text, chat_type: 7 }
                .encode()
                .unwrap()
        );
        assert!(!runtime.portals.has_pending());
    }
    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}

#[tokio::test]
async fn committed_link_drains_without_private_packet_for_disconnected_or_replaced_session() {
    let (_cluster, _directory, mut runtime, key, binding) =
        crate::game_runtime::portals::tests::output_runtime().await;
    runtime.limits.messages = 0;
    runtime.sessions.get_mut(&key).unwrap().terminated = true;
    runtime.sessions.get_mut(&key).unwrap().disconnected = true;
    stage_link(&mut runtime, binding, link_ticket(161, binding.actor, 15));
    runtime.project_portal_deliveries().unwrap();
    assert!(runtime.network_output.is_empty());
    assert!(!runtime.portals.has_pending());

    // A later replica for the same actor has a different authenticated
    // binding. This is only an output-state-machine fixture, not admission.
    runtime.sessions.get_mut(&key).unwrap().terminated = false;
    runtime.sessions.get_mut(&key).unwrap().disconnected = false;
    runtime
        .players
        .replication(binding.actor)
        .unwrap()
        .binding
        .session = bace_gameplay_api::SessionId(binding.session.0 + 1);
    stage_link(&mut runtime, binding, link_ticket(162, binding.actor, 15));
    runtime.project_portal_deliveries().unwrap();
    assert!(runtime.network_output.is_empty());
    assert!(!runtime.portals.has_pending());
    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}

#[tokio::test]
async fn link_with_missing_or_mismatched_captured_binding_remains_retained() {
    let (_cluster, _directory, mut runtime, key, binding) =
        crate::game_runtime::portals::tests::output_runtime().await;
    runtime.limits.messages = 0;
    stage_link(&mut runtime, binding, link_ticket(171, binding.actor, 15));
    runtime.portals.ticket_bindings.remove(&171);
    assert!(
        runtime
            .project_portal_deliveries()
            .unwrap_err()
            .contains("binding missing")
    );
    assert!(runtime.portals.output_pending());
    assert!(runtime.network_output.is_empty());
    let mut wrong = binding;
    wrong.actor = EntityId(binding.actor.0 + 1);
    runtime.portals.ticket_bindings.insert(171, vec![wrong]);
    assert!(
        runtime
            .project_portal_deliveries()
            .unwrap_err()
            .contains("identity mismatch")
    );
    assert!(runtime.portals.output_pending());
    runtime.portals.ticket_bindings.insert(171, vec![binding]);
    runtime.limits.messages = 1;
    runtime.portals.tickets.get_mut(&171).unwrap().effect =
        link_ticket(171, binding.actor, 4).effect;
    assert!(
        runtime
            .project_portal_deliveries()
            .unwrap_err()
            .contains("source slot")
    );
    assert!(runtime.portals.output_pending());
    assert!(runtime.network_output.is_empty());
    // Test teardown only: no invalid effect or captured identity is drained by
    // the live owner.
    while let Some(delivery) = runtime.portals.deliveries.front() {
        let sequence = delivery.sequence;
        runtime.portals.acknowledge(sequence).unwrap();
    }
    runtime.portals.tickets.remove(&171);
    runtime.portals.ticket_bindings.remove(&171);
    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}
