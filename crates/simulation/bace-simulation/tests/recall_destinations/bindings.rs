//! Synthetic simulation-owner qualification of authenticated stone Use. The
//! original ACE sound/message vectors are covered by the runtime projection.
use super::*;
use bace_interactions::BindingKind;
use bace_simulation::{
    PortalServiceEffect, PortalServiceOrigin, PortalServiceReceipt, PreparedBindingObject,
};

fn stone(kind: BindingKind, radius: f32) -> PreparedBindingObject {
    PreparedBindingObject {
        entity: EntityId(2),
        kind,
        use_radius: radius,
        use_message: Arc::from("You have attuned to the Lifestone."),
    }
}

fn use_stone(k: &mut Kernel, sequence: u32, revision: u64) {
    let chain = super::recall_motion::binding_motion(3);
    k.apply_recall_command(RecallCommand::UseBinding {
        context: context(sequence),
        object: EntityId(2),
        before_revision: revision,
        animation_seconds: chain.nominal_duration_seconds(),
        style: None,
        motion: chain,
    })
    .unwrap();
}

#[test]
fn live_stone_use_requires_authenticated_owner_range_and_cancel_clears_action() {
    let mut k = prepared();
    let revision = k.character(EntityId(1)).unwrap().revision();
    k.register_binding_object(stone(BindingKind::Lifestone, 0.1))
        .unwrap();
    use_stone(&mut k, 1, revision);
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::Rejected {
            error: RecallError::MovedTooFar,
            ..
        })
    ));
    k.unregister_binding_objects(&[EntityId(2)]).unwrap();
    k.register_binding_object(stone(BindingKind::Lifestone, 5.0))
        .unwrap();
    let mut stale = context(2);
    stale.session = SessionId(99);
    let chain = super::recall_motion::binding_motion(3);
    k.apply_recall_command(RecallCommand::UseBinding {
        context: stale,
        object: EntityId(2),
        before_revision: revision,
        animation_seconds: chain.nominal_duration_seconds(),
        style: None,
        motion: chain,
    })
    .unwrap();
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::Rejected { context, error: RecallError::Stale }) if context == stale
    ));
    let stone_pose = k.world().actor_state(EntityId(2)).unwrap().1.position();
    use_stone(&mut k, 3, revision);
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::BindingStarted {
            object: EntityId(2),
            ..
        })
    ));
    assert_eq!(
        k.world().source_motion_token(EntityId(1)).unwrap().domain,
        bace_motion::MotionDomain::Recall
    );
    k.apply_recall_command(RecallCommand::Cancel { actor: EntityId(1) })
        .unwrap();
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::Cancelled { .. })
    ));
    for _ in 0..10 {
        step(&mut k);
    }
    assert!(k.take_portal_proposal().is_none());
    assert_eq!(
        k.world().actor_state(EntityId(2)).unwrap().1.position(),
        stone_pose
    );
}

#[test]
fn live_lifestone_stages_exact_sanctuary_and_stamina_then_adopts_receipt() {
    let mut k = prepared();
    k.register_npc_character_services(
        EntityId(1),
        bace_character::CharacterServiceState {
            level: 1,
            total_experience: 0,
            titles: vec![],
            enlightenment: 0,
            sanctuary: None,
            total_skill_credits: None,
        },
        bace_quests::ContractRegistry::restore(vec![]).unwrap(),
    )
    .unwrap();
    k.register_binding_object(stone(BindingKind::Lifestone, 5.0))
        .unwrap();
    let revision = k.character(EntityId(1)).unwrap().revision();
    let stone_pose = k.world().actor_state(EntityId(2)).unwrap().1.position();
    let player_pose = k.world().actor_state(EntityId(1)).unwrap().1.position();
    use_stone(&mut k, 4, revision);
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::BindingStarted { .. })
    ));
    for _ in 0..12 {
        step(&mut k);
        if k.pending_portal_proposal(1).is_some() {
            break;
        }
    }
    let staged = k.take_recall_event().expect("binding stage");
    let operation = match staged {
        RecallEvent::BindingStaged {
            operation,
            object,
            allegiance: false,
            use_message,
            stamina_after: Some(50),
            ..
        } => {
            assert_eq!(object, EntityId(2));
            assert_eq!(use_message.as_ref(), "You have attuned to the Lifestone.");
            operation
        }
        other => panic!("unexpected binding event: {other:?}"),
    };
    let ticket = k.take_portal_proposal().expect("exact durable proposal");
    assert_eq!(ticket.operation, operation);
    assert_eq!(ticket.origin, PortalServiceOrigin::Binding);
    let PortalServiceEffect::Sanctuary {
        link,
        character,
        stamina,
    } = &ticket.effect
    else {
        panic!("sanctuary owner");
    };
    assert_eq!(
        link.after.origin,
        [player_pose.x, player_pose.y, player_pose.z]
    );
    assert_eq!(stamina.before, 100);
    assert_eq!(stamina.after, 50);
    assert!(character.before.sanctuary.is_none());
    assert_eq!(
        k.world().actor_state(EntityId(2)).unwrap().1.position(),
        stone_pose
    );
    k.confirm_portal_committed(&PortalServiceReceipt {
        operation,
        actor: ticket.actor,
        after_revision: ticket.after_revision,
        revisions: ticket
            .participants
            .iter()
            .map(|(id, _, after)| (*id, *after))
            .collect(),
    })
    .unwrap();
    step(&mut k);
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Stamina)
            .unwrap()
            .current,
        50
    );
    assert_eq!(
        k.portal_links(EntityId(1)).unwrap().position(4),
        Some(link.after)
    );
    assert_eq!(
        k.world().actor_state(EntityId(2)).unwrap().1.position(),
        stone_pose
    );
}

#[test]
fn allegiance_bindstone_stages_monarch_sanctuary_without_stamina_debit() {
    let mut k = prepared();
    let monarch = bace_allegiance::AllegianceNode {
        character: EntityId(1),
        account: AccountId(1),
        name: "Alice".into(),
        gender: 1,
        heritage: 1,
        patron: None,
        monarch: EntityId(1),
        vassals: vec![],
        rank: 1,
        followers: 0,
        level: 1,
        leadership: 7,
        loyalty: 8,
        sworn_at: 0,
        online_seconds: 1,
        may_pass_up: false,
        received_total: 0,
        tithed_total: 0,
        unclaimed: 0,
    };
    k.register_allegiances(
        bace_allegiance::AllegianceRegistry::restore(
            vec![monarch],
            vec![bace_allegiance::AllegianceMetadata::new(
                EntityId(1),
                0x8000_0001,
            )],
            0,
            8,
        )
        .unwrap(),
        0.,
    )
    .unwrap();
    k.register_binding_object(stone(BindingKind::Allegiance, 5.0))
        .unwrap();
    let stone_pose = k.world().actor_state(EntityId(2)).unwrap().1.position();
    let player_pose = k.world().actor_state(EntityId(1)).unwrap().1.position();
    let stamina = k
        .world()
        .vital(EntityId(1), EntityVital::Stamina)
        .unwrap()
        .current;
    let revision = k.character(EntityId(1)).unwrap().revision();
    use_stone(&mut k, 8, revision);
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::BindingStarted { .. })
    ));
    for _ in 0..12 {
        step(&mut k);
    }
    let staged = k.take_recall_event().expect("allegiance binding stage");
    let operation = match staged {
        RecallEvent::BindingStaged {
            operation,
            allegiance: true,
            stamina_after: None,
            ..
        } => operation,
        other => panic!("unexpected allegiance binding event: {other:?}"),
    };
    let ticket = k
        .take_allegiance_proposal()
        .expect("allegiance durable proposal");
    assert_eq!(ticket.operation, operation);
    assert_eq!(
        ticket.patch.metadata[0]
            .1
            .as_ref()
            .unwrap()
            .sanctuary
            .unwrap()
            .origin,
        [player_pose.x, player_pose.y, player_pose.z]
    );
    k.confirm_allegiance_committed(&ticket).unwrap();
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Stamina)
            .unwrap()
            .current,
        stamina
    );
    assert_eq!(
        k.world().actor_state(EntityId(2)).unwrap().1.position(),
        stone_pose
    );
}
