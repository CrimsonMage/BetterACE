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
    }
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
