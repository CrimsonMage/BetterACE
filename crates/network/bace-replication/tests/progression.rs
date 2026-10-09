use bace_gameplay_api::*;
use bace_replication::*;
use bace_types::{AccountId, EntityId};
fn context() -> ActionContext {
    ActionContext {
        session: SessionId(1),
        account: AccountId(2),
        actor: EntityId(3),
        sequence: 1,
    }
}
fn projector(capacity: usize) -> ProgressionProjector {
    let c = context();
    ProgressionProjector::new(
        CharacterBinding {
            session: c.session,
            account: c.account,
            actor: c.actor,
        },
        0,
        capacity,
    )
    .unwrap()
}
fn change() -> ProgressionChange {
    let before = ProgressionProjection {
        target: ProgressionTarget::Attribute(AttributeId::Strength),
        experience_spent: 0,
        ranks: 0,
        advancement: SkillAdvancement::Inactive,
        details: Some(TraitDetails::Attribute {
            starting_value: 100,
        }),
    };
    ProgressionChange {
        before,
        after: ProgressionProjection {
            experience_spent: 10,
            ranks: 1,
            ..before
        },
        available_experience: 90,
        revision: 1,
        rank_effect: None,
        follow_up_vital: None,
    }
}

#[test]
fn ace_rank_effect_uses_frozen_base_and_source_output_order() {
    // Pinned ACE Player_Attributes.cs, Player_Vitals.cs, Player_Skills.cs:
    // each Raise handler emits a max-rank WeddingBliss script, RaiseTrait
    // sound, then Advancement chat after the private trait update.
    let limits = BatchLimits {
        max_messages: 8,
        max_bytes: 4096,
        max_message_bytes: 4096,
        max_string_bytes: 4096,
    };
    let mut raised = change();
    raised.rank_effect = Some(RankEffect {
        base: 101,
        reached_maximum: true,
    });
    let packets = project_rank_effect(3, raised, limits).unwrap().unwrap();
    assert_eq!(
        packets.owner.iter().map(|m| m.queue).collect::<Vec<_>>(),
        [10, 10, 9]
    );
    assert_eq!(
        packets.owner[0].bytes,
        [
            0x55, 0xf7, 0, 0, 3, 0, 0, 0, 0x8d, 0, 0, 0, 0, 0, 0x80, 0x3f,
        ]
    );
    assert_eq!(packets.observers, packets.owner[..1]);
    assert_eq!(
        packets.owner[1].bytes,
        [
            0x50, 0xf7, 0, 0, 3, 0, 0, 0, 0x8b, 0, 0, 0, 0, 0, 0x80, 0x3f,
        ]
    );
    let fixture = include_str!("fixtures/rank_effect.hex").trim();
    let golden = (0..fixture.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&fixture[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(packets.owner[2].bytes, golden);
    assert_eq!(
        packets.owner[2].bytes,
        bace_wire::ChatMessage::System {
            text: "Your base Strength is now 101 and has reached its upper limit!",
            chat_type: 13,
        }
        .encode()
        .unwrap()
    );
    raised.after.target = ProgressionTarget::Vital(VitalId::MaxHealth);
    raised.before.target = raised.after.target;
    raised.rank_effect = Some(RankEffect {
        base: 125,
        reached_maximum: false,
    });
    let vital = project_rank_effect(3, raised, limits).unwrap().unwrap();
    assert_eq!(vital.owner.len(), 2);
    assert_eq!(vital.observers.len(), 0);
    assert_eq!(
        vital.owner[1].bytes,
        bace_wire::ChatMessage::System {
            text: "Your base Maximum Health is now 125!",
            chat_type: 13,
        }
        .encode()
        .unwrap()
    );
    raised.after.target = ProgressionTarget::Skill(24);
    raised.before.target = raised.after.target;
    let skill = project_rank_effect(3, raised, limits).unwrap().unwrap();
    assert_eq!(
        skill.owner[1].bytes,
        bace_wire::ChatMessage::System {
            text: "Your base Run skill is now 125!",
            chat_type: 13,
        }
        .encode()
        .unwrap()
    );
}

#[test]
fn endurance_health_follow_up_uses_accepted_current_and_atomic_vital_sequence() {
    let mut raised = change();
    raised.before.target = ProgressionTarget::Attribute(AttributeId::Endurance);
    raised.after.target = raised.before.target;
    raised.follow_up_vital = Some(ProgressionProjection {
        target: ProgressionTarget::Vital(VitalId::MaxHealth),
        experience_spent: 31,
        ranks: 2,
        advancement: SkillAdvancement::Inactive,
        details: Some(TraitDetails::Vital {
            starting_value: 100,
            current: 47,
        }),
    });
    let mut insufficient = projector(2);
    assert!(matches!(
        insufficient.project(context(), raised),
        Err(ProgressionProjectionError::Capacity)
    ));
    let after_rejection = insufficient.project(context(), change()).unwrap();
    assert_eq!(after_rejection.messages[0][4], 0);
    assert_eq!(after_rejection.messages[1][4], 0);

    let mut projector = projector(3);
    let mut invalid = raised;
    invalid.follow_up_vital.as_mut().unwrap().target = ProgressionTarget::Vital(VitalId::MaxMana);
    assert!(matches!(
        projector.project(context(), invalid),
        Err(ProgressionProjectionError::InvalidProjection)
    ));
    let packets = projector.project(context(), raised).unwrap();
    assert_eq!(packets.messages[0][4], 0);
    assert_eq!(packets.messages[1][4], 0);
    assert_eq!(
        packets.follow_up_vital.unwrap(),
        bace_wire::VitalUpdate {
            sequence: 0,
            object_id: None,
            vital: 1,
            ranks: 2,
            starting_value: 100,
            experience_spent: 31,
            current: 47,
        }
        .encode()
    );
}
#[test]
fn missing_metadata_and_wrong_identity_do_not_consume_revision_or_counters() {
    let mut projector = projector(2);
    let mut missing = change();
    missing.after.details = None;
    assert!(matches!(
        projector.project(context(), missing),
        Err(ProgressionProjectionError::MissingDetails)
    ));
    assert!(matches!(
        projector.project(
            ActionContext {
                account: AccountId(9),
                ..context()
            },
            change()
        ),
        Err(ProgressionProjectionError::WrongBinding)
    ));
    let packets = projector.project(context(), change()).unwrap();
    assert_eq!(packets.messages[0][4], 0);
    assert_eq!(packets.messages[1][4], 0);
    assert_eq!(packets.queue, 9);
    assert!(packets.rank_changed);
    assert!(matches!(
        projector.project(context(), change()),
        Err(ProgressionProjectionError::StaleRevision)
    ));
}
#[test]
fn failed_multi_counter_reservation_is_atomic() {
    let mut sequences = Sequences::new(1).unwrap();
    assert_eq!(
        sequences.advance_batch([
            (SequenceKind::PropertyInt64, 2),
            (SequenceKind::Attribute, 1)
        ]),
        Err(ReplicationError::Capacity)
    );
    assert_eq!(sequences.advance(SequenceKind::Attribute, 1).unwrap(), 0);
    assert_eq!(
        sequences
            .advance_batch([(SequenceKind::Attribute, 1), (SequenceKind::Attribute, 1)])
            .unwrap(),
        [1, 2]
    );
    assert!(matches!(
        projector(1).project(context(), change()),
        Err(ProgressionProjectionError::Capacity)
    ));
}
#[test]
fn inconsistent_or_nonfinite_authoritative_views_are_errors() {
    let mut projector = projector(2);
    let mut wrong = change();
    wrong.after.details = Some(TraitDetails::Vital {
        starting_value: 1,
        current: 1,
    });
    assert!(matches!(
        projector.project(context(), wrong),
        Err(ProgressionProjectionError::InvalidProjection)
    ));
    let mut wrong = change();
    wrong.available_experience = u64::MAX;
    assert!(matches!(
        projector.project(context(), wrong),
        Err(ProgressionProjectionError::InvalidProjection)
    ));
    let mut wrong = change();
    wrong.before.target = ProgressionTarget::Skill(10);
    wrong.after.target = wrong.before.target;
    wrong.after.advancement = SkillAdvancement::Trained;
    wrong.after.details = Some(TraitDetails::Skill {
        initial_level: 5,
        resistance_at_last_check: 0,
        last_used_time: f64::NAN,
    });
    assert!(matches!(
        projector.project(context(), wrong),
        Err(ProgressionProjectionError::InvalidProjection)
    ));
    assert!(projector.project(context(), change()).is_ok());
}

#[test]
fn legitimate_zero_xp_actions_emit_primary_updates_without_replaying_duplicates() {
    let mut projector = projector(2);
    let mut zero = change();
    zero.after = zero.before;
    zero.available_experience = 100;
    zero.revision = 0;
    let first = projector.project(context(), zero).unwrap();
    assert_eq!(first.revision, 0);
    assert!(!first.rank_changed);
    assert_eq!(first.messages[0][4], 0);
    let next = ActionContext {
        sequence: 2,
        ..context()
    };
    let second = projector.project(next, zero).unwrap();
    assert_eq!(second.messages[0][4], 1);
    assert!(matches!(
        projector.project(next, zero),
        Err(ProgressionProjectionError::StaleSequence)
    ));
    let positive = projector
        .project(
            ActionContext {
                sequence: 3,
                ..context()
            },
            change(),
        )
        .unwrap();
    assert_eq!(positive.messages[0][4], 2);
}

#[test]
fn training_class_message_and_full_unspec_share_skill_sequence_without_consuming_absent_xp() {
    let before = ProgressionProjection {
        target: ProgressionTarget::Skill(28),
        experience_spent: 0,
        ranks: 0,
        advancement: SkillAdvancement::Untrained,
        details: Some(TraitDetails::Skill {
            initial_level: 0,
            resistance_at_last_check: 0,
            last_used_time: 0.0,
        }),
    };
    let trained = ProgressionProjection {
        advancement: SkillAdvancement::Trained,
        ..before
    };
    let change = SkillTrainingChange {
        before,
        after: trained,
        available_skill_credits: 10,
        revision: 1,
    };
    // Training only needs the skill and credit sequence keys, not an XP key.
    let mut minimal = projector(2);
    let packets = minimal.project_training(context(), change, None).unwrap();
    assert_eq!(
        packets.messages[0],
        [0xe2, 2, 0, 0, 0, 3, 0, 0, 0, 28, 0, 0, 0, 2, 0, 0, 0]
    );
    assert_eq!(packets.messages.len(), 2);
    let mut full = projector(3);
    full.project_training(context(), change, None).unwrap();
    let after = ProgressionProjection {
        ranks: 2,
        experience_spent: 10,
        ..trained
    };
    let packets = full
        .project_training(
            ActionContext {
                sequence: 2,
                ..context()
            },
            SkillTrainingChange {
                before: trained,
                after,
                available_skill_credits: 10,
                revision: 2,
            },
            Some(90),
        )
        .unwrap();
    assert_eq!(packets.messages.len(), 4);
    assert_eq!(packets.messages[0][4], 1);
    assert_eq!(packets.messages[1][4], 2);
    assert_eq!(packets.messages[3][4], 0);
}

#[test]
fn live_cursor_uses_existing_session_counters_and_keeps_rejections_atomic() {
    let context = context();
    let binding = CharacterBinding {
        actor: context.actor,
        session: context.session,
        account: context.account,
    };
    let mut cursor = ProgressionCursor::new(binding, 0);
    let mut sequences = Sequences::with_instance(4, 17).unwrap();
    // Entry/other adapters already emitted updates on this same session.
    for _ in 0..7 {
        sequences.advance(SequenceKind::PropertyInt64, 2).unwrap();
        sequences.advance(SequenceKind::Attribute, 1).unwrap();
    }
    let mut invalid = change();
    invalid.after.details = None;
    assert!(matches!(
        cursor.project(&mut sequences, context, invalid),
        Err(ProgressionProjectionError::MissingDetails)
    ));
    assert_eq!(sequences.current(SequenceKind::PropertyInt64, 2), 6);
    let packets = cursor.project(&mut sequences, context, change()).unwrap();
    assert_eq!(packets.messages[0][4], 7);
    assert_eq!(packets.messages[1][4], 7);
    assert_eq!(sequences.current(SequenceKind::ObjectInstance, 0), 17);
    assert!(matches!(
        cursor.project(&mut sequences, context, change()),
        Err(ProgressionProjectionError::StaleRevision)
    ));
    assert_eq!(sequences.current(SequenceKind::PropertyInt64, 2), 7);
}
