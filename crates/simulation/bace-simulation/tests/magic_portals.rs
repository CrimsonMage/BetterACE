mod magic_common;
use bace_interactions::{PortalAccess, PortalLinks, PortalPosition};
use bace_simulation::{PortalServiceEffect, PortalServiceEvent, PortalServiceReceipt};
use magic_common::*;
fn access() -> PortalAccess {
    PortalAccess {
        level: 275,
        pk_status: 2,
        pk_recent: false,
        olthoi: false,
        vitae: false,
        account_15_days: true,
        entitlement: u32::MAX,
        quest_allowed: true,
        teleporting: false,
        recently_teleported: false,
        ignore_restrictions: false,
        enforce_maximum_level: true,
    }
}
#[test]
fn trusted_player_transfer_cleans_awaiting_portal_without_client_readiness() {
    let mut k = kernel(64);
    let actor = EntityId(1);
    let destination = PortalPosition {
        cell: 1,
        origin: [4., 4., 0.5],
        rotation: [1., 0., 0., 0.],
    };
    k.register_portal_links(
        actor,
        PortalLinks::new(0, &[(15, destination), (4, destination)], &[]).unwrap(),
        access(),
    )
    .unwrap();
    k.register_magic_spell(spell(
        100,
        SpellEffect::Portal(bace_magic::PortalEffect::Recall { slot: 2 }),
    ))
    .unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: actor,
            spell: 100,
        },
    })
    .unwrap();
    for _ in 0..8 {
        step(&mut k);
    }
    let ticket = k.take_portal_proposal().unwrap();
    k.confirm_portal_committed(&PortalServiceReceipt {
        operation: ticket.operation,
        actor: ticket.actor,
        after_revision: ticket.after_revision,
        revisions: ticket
            .participants
            .iter()
            .map(|&(id, _, after)| (id, after))
            .collect(),
    })
    .unwrap();
    for _ in 0..64 {
        step(&mut k);
    }
    assert!(k.world().is_in_portal_transit(actor));
    let events: Vec<_> = std::iter::from_fn(|| k.take_portal_event()).collect();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, PortalServiceEvent::Teleported { .. }))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, PortalServiceEvent::Materialized { .. }))
    );
    let state = k
        .take_player_state(CharacterBinding {
            actor,
            account: AccountId(1),
            session: SessionId(7),
        })
        .unwrap();
    assert_eq!(state.portal_links.unwrap().position(15), Some(destination));
    assert!(!k.world().is_in_portal_transit(actor));
    assert!(!k.has_portal_state());
    assert!(
        k.take_portal_event().is_none(),
        "lifecycle cleanup does not fabricate materialization"
    );
}
#[test]
fn recall_commits_mana_then_waits_for_delay_and_authenticated_ready_without_anchor_alias() {
    let mut k = kernel(64);
    let anchor = PortalPosition {
        cell: 1,
        origin: [4.0, 4.0, 0.5],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let stone = EntityId(50);
    let body = Body::spawn(
        k.world().scene(CellId(anchor.cell)).unwrap(),
        Vec3::new(anchor.origin[0], anchor.origin[1], anchor.origin[2]),
        0.5,
        Capabilities {
            speed: 0.0,
            jump_impulse: 0.0,
        },
    )
    .unwrap();
    k.register_portal_anchor(
        bace_interactions::PortalAnchor {
            entity: stone.0,
            template: 123,
            original_template: None,
            kind: bace_interactions::PortalKind::Lifestone,
            position: anchor,
            destination: None,
            minimum_level: 0,
            maximum_level: 0,
            restrictions: 0,
            no_tie: false,
            ignore_pk_timer: false,
            account_requirement: 0,
        },
        Actor {
            id: stone,
            cell: CellId(anchor.cell),
            body,
        },
    )
    .map_err(|(error, _)| error)
    .unwrap();
    let stone_before = k.world().actor_state(stone).unwrap().1.position();
    k.register_portal_links(
        EntityId(1),
        PortalLinks::new(0, &[(15, anchor), (4, anchor)], &[]).unwrap(),
        access(),
    )
    .unwrap();
    k.register_magic_spell(spell(
        100,
        SpellEffect::Portal(bace_magic::PortalEffect::Recall { slot: 2 }),
    ))
    .unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    for _ in 0..8 {
        step(&mut k);
    }
    let ticket = k.take_portal_proposal().expect("prepared durable recall");
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(7),
    };
    assert!(k.read_player_snapshot(binding).is_err());
    assert!(
        k.read_player_operation_snapshot(
            binding,
            bace_simulation::PlayerSnapshotOperation::Portal(ticket.operation + 1),
            ticket.before_revision
        )
        .is_err()
    );
    let before = k
        .read_player_operation_snapshot(
            binding,
            bace_simulation::PlayerSnapshotOperation::Portal(ticket.operation),
            ticket.before_revision,
        )
        .unwrap();
    assert_eq!(
        before.character().progression().revision(),
        ticket.before_revision
    );
    assert_eq!(
        k.apply_ui(context(2), bace_gameplay_api::UiRequest::Filters(3)),
        Err(bace_gameplay_api::UiError::DurabilityPending)
    );
    k.enqueue(Command::RaiseProgression {
        context: context(2),
        request: bace_gameplay_api::RaiseProgression {
            target: bace_gameplay_api::ProgressionTarget::Attribute(
                bace_gameplay_api::AttributeId::Strength,
            ),
            amount: 10,
        },
    })
    .unwrap();
    for _ in 0..90 {
        step(&mut k);
    }
    assert_eq!(
        k.take_progression_outcome().unwrap().result,
        Err(bace_gameplay_api::ProgressionActionRejection::DurabilityPending)
    );
    assert_eq!(
        k.read_player_operation_snapshot(
            binding,
            bace_simulation::PlayerSnapshotOperation::Portal(ticket.operation),
            ticket.before_revision
        )
        .unwrap()
        .character()
        .progression()
        .revision(),
        ticket.before_revision
    );

    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    assert_eq!(ticket.mana.unwrap().after, 90);
    assert!(k.world().vital_reserved(EntityId(1), EntityVital::Mana));
    let mut receipt = PortalServiceReceipt {
        operation: ticket.operation,
        actor: ticket.actor,
        after_revision: ticket.after_revision,
        revisions: ticket
            .participants
            .iter()
            .map(|&(id, _, after)| (id, after))
            .collect(),
    };
    receipt.after_revision += 1;
    assert!(k.confirm_portal_committed(&receipt).is_err());
    receipt.after_revision -= 1;
    k.confirm_portal_committed(&receipt).unwrap();
    assert!(k.world().vital_reserved(EntityId(1), EntityVital::Mana));
    step(&mut k);
    assert!(!k.world().vital_reserved(EntityId(1), EntityVital::Mana));
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert_eq!(
        k.world().actor_state(EntityId(1)).unwrap().1.position().x,
        0.0
    );
    for _ in 0..62 {
        step(&mut k);
    }
    let state = k.world().actor_state(EntityId(1)).unwrap().1;
    assert!((state.position().z - 0.505).abs() < 0.000001);
    assert_eq!(state.position().x, 4.0);
    assert_eq!(
        k.portal_links(EntityId(1)).unwrap().position(15),
        Some(anchor)
    );
    assert!(k.world().is_in_portal_transit(EntityId(1)));
    let rejected = k.apply_portal_resolution(bace_simulation::PortalResolutionCommand {
        correlation: 51,
        resolution: bace_simulation::PortalResolution::ClientReady {
            context: context(2),
            operation: ticket.operation,
            epoch: state.epoch().wrapping_add(1),
        },
    });
    assert!(
        rejected.result.is_err(),
        "a different accepted epoch cannot materialize"
    );
    let accepted = k.apply_portal_resolution(bace_simulation::PortalResolutionCommand {
        correlation: 52,
        resolution: bace_simulation::PortalResolution::ClientReady {
            context: context(2),
            operation: ticket.operation,
            epoch: state.epoch(),
        },
    });
    assert!(
        accepted.result.is_ok(),
        "rejected readiness did not consume the client sequence"
    );
    assert!(k.world().is_in_portal_transit(EntityId(1)));
    assert!(
        k.mark_portal_destination_ready(
            EntityId(1),
            ticket.operation,
            state.epoch().wrapping_add(1)
        )
        .is_err()
    );
    assert!(
        k.apply_portal_resolution(bace_simulation::PortalResolutionCommand {
            correlation: 53,
            resolution: bace_simulation::PortalResolution::DestinationReady {
                actor: EntityId(1),
                operation: ticket.operation,
                epoch: state.epoch(),
            },
        })
        .result
        .is_ok()
    );
    assert!(!k.world().is_in_portal_transit(EntityId(1)));
    let events: Vec<_> = std::iter::from_fn(|| k.take_portal_event()).collect();
    let frozen = events
        .iter()
        .find_map(|event| match event {
            PortalServiceEvent::Teleported { views, .. } => Some(views.clone()),
            _ => None,
        })
        .expect("teleport retains its accepted output evidence");
    assert_eq!(frozen.len(), 1);
    assert_eq!(frozen[0].actor, EntityId(1));
    assert_eq!(
        frozen[0].position.origin,
        [state.position().x, state.position().y, state.position().z]
    );
    assert_eq!(frozen[0].epoch, state.epoch());

    assert!(
        events
            .iter()
            .any(|e| matches!(e, PortalServiceEvent::Hidden { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, PortalServiceEvent::Teleported { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, PortalServiceEvent::Materialized { .. }))
    );
    assert!(
        k.read_player_snapshot(binding).is_ok(),
        "character/registry owner still held"
    );
    assert!(!k.portal_reserved(EntityId(1)), "portal owner still held");
    assert!(
        !k.world().has_pending_action_motion(EntityId(1)),
        "action motion still pending"
    );
    // Let the accepted return gesture finish after the extended durable hold.
    for _ in 0..8 {
        step(&mut k);
    }
    k.enqueue(Command::Cast {
        context: context(3),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    for _ in 0..8 {
        step(&mut k);
    }
    let second = k.take_portal_proposal().unwrap_or_else(|| {
        panic!(
            "second recall: {:?}",
            std::iter::from_fn(|| k.take_cast_outcome()).collect::<Vec<_>>()
        )
    });
    assert!(
        matches!(&second.effect,PortalServiceEffect::Teleport(moves) if (moves[0].position.z-0.505).abs()<0.000001)
    );
    assert!(k.world().vital_reserved(EntityId(1), EntityVital::Mana));
    for _ in 0..90 {
        step(&mut k);
    }
    k.reject_portal_proposal(second.operation).unwrap();
    assert!(!k.world().has_vital_reservations());
    assert_eq!(
        k.portal_links(EntityId(1)).unwrap().position(15),
        Some(anchor)
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert!(k.read_player_snapshot(binding).is_ok());
    assert!(!k.portal_reserved(EntityId(1)));
    for _ in 0..8 {
        step(&mut k);
    }
    k.enqueue(Command::Cast {
        context: context(4),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    for _ in 0..8 {
        step(&mut k);
    }
    assert!(
        k.take_portal_proposal().is_some(),
        "rejected delayed recall left caster busy"
    );
    assert_eq!(
        k.world().actor_state(stone).unwrap().1.position(),
        stone_before
    );
    assert_eq!(stone_before, Vec3::new(4.0, 4.0, 0.5));
    assert_eq!(
        k.portal_links(EntityId(1)).unwrap().position(4),
        Some(anchor)
    );
}
