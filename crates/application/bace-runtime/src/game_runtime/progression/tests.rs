use super::*;
use bace_gameplay_api::SessionId;
use bace_types::{AccountId, EntityId};
use bace_wire::{GameActionEnvelope, opcode::GameActionType};
fn context() -> ActionContext {
    ActionContext {
        actor: EntityId(3),
        account: AccountId(2),
        session: SessionId(7),
        sequence: 99,
    }
}
fn key() -> SessionKey {
    SessionKey {
        id: 1,
        generation: 7,
    }
}
fn packet(action: GameActionType, payload: &[u8]) -> Vec<u8> {
    GameActionEnvelope {
        sequence: 4,
        action,
        payload,
    }
    .encode(4096)
    .unwrap()
}
#[test]
fn admitted_training_keeps_durable_identity_and_reliable_sequence_on_retry() {
    let payload = [1u32.to_le_bytes(), 6u32.to_le_bytes()].concat();
    let bytes = packet(GameActionType::TrainSkill, &payload);
    assert!(matches!(
        decode_action(context(), &bytes, 4096, true).unwrap(),
        Some(None)
    ));
    let action = decode_action(context(), &bytes, 4096, false)
        .unwrap()
        .unwrap()
        .unwrap();
    let Action::Train(_, identity) = action else {
        panic!("training action");
    };
    let mut runtime = ProgressionRuntime::new();
    runtime.pending.insert(
        key(),
        Pending {
            context: context(),
            action,
            phase: Phase::Submitted,
        },
    );
    assert!(runtime.training_busy());
    runtime
        .accept_skill(bace_gameplay_api::ActionResult {
            context: context(),
            result: Err(SkillActionError::Busy),
        })
        .unwrap();
    let pending = &runtime.pending[&key()];
    assert!(matches!(pending.phase, Phase::Queued));
    assert!(matches!(pending.action, Action::Train(_, id) if id == identity));
    assert!(
        matches!(pending.action.command(pending.context), Command::TrainSkill { context: c, .. } if c.sequence == 99)
    );
    runtime.pending.get_mut(&key()).unwrap().phase = Phase::Submitted;
    runtime
        .accept_skill(bace_gameplay_api::ActionResult {
            context: context(),
            result: Err(SkillActionError::Profile),
        })
        .unwrap();
    assert!(!runtime.has_pending());
    assert!(runtime.failures.contains_key(&key()));
}
#[test]
fn ui_defers_only_pre_authorization_holds_and_keeps_other_sessions_distinct() {
    let bytes = packet(GameActionType::SpellbookFilter, &13u32.to_le_bytes());
    let action = decode_action(context(), &bytes, 4096, false)
        .unwrap()
        .unwrap()
        .unwrap();
    let mut runtime = ProgressionRuntime::new();
    runtime.pending.insert(
        key(),
        Pending {
            context: context(),
            action,
            phase: Phase::Submitted,
        },
    );
    let wrong = UiOutcome {
        context: ActionContext {
            session: SessionId(8),
            ..context()
        },
        result: Ok(2),
    };
    assert_eq!(runtime.accept_ui(wrong.clone()), Err(wrong));
    assert!(matches!(runtime.pending[&key()].phase, Phase::Submitted));
    runtime
        .accept_ui(UiOutcome {
            context: context(),
            result: Err(bace_gameplay_api::UiError::DurabilityPending),
        })
        .unwrap();
    assert!(matches!(runtime.pending[&key()].phase, Phase::Queued));
    runtime.pending.get_mut(&key()).unwrap().phase = Phase::Submitted;
    runtime
        .accept_ui(UiOutcome {
            context: context(),
            result: Ok(3),
        })
        .unwrap();
    assert!(!runtime.has_pending());
    assert!(runtime.failures.is_empty());
}
#[test]
fn malformed_known_input_is_not_silently_forwarded_and_progression_rejection_is_terminal() {
    assert!(
        decode_action(
            context(),
            &packet(GameActionType::TrainSkill, &[]),
            4096,
            true
        )
        .is_err()
    );
    assert!(
        decode_action(
            context(),
            &packet(GameActionType::SpellbookFilter, &[1]),
            4096,
            false
        )
        .is_err()
    );
    let mut runtime = ProgressionRuntime::new();
    runtime.pending.insert(
        key(),
        Pending {
            context: context(),
            action: Action::Raise(RaiseProgression {
                target: bace_gameplay_api::ProgressionTarget::Skill(1),
                amount: 10,
            }),
            phase: Phase::Submitted,
        },
    );
    runtime
        .accept_raise(bace_gameplay_api::ActionResult {
            context: context(),
            result: Err(ProgressionActionRejection::DurabilityPending),
        })
        .unwrap();
    assert!(matches!(runtime.pending[&key()].phase, Phase::Queued));
    runtime.pending.get_mut(&key()).unwrap().phase = Phase::Submitted;
    runtime
        .accept_raise(bace_gameplay_api::ActionResult {
            context: context(),
            result: Err(ProgressionActionRejection::Domain(
                bace_gameplay_api::ProgressionRejection::InsufficientExperience,
            )),
        })
        .unwrap();
    assert!(!runtime.has_pending());
    assert!(runtime.failures.contains_key(&key()));
}

#[test]
fn persistent_hold_on_lowest_session_does_not_starve_higher_sessions() {
    let mut pending = BTreeMap::new();
    for id in 1..=4 {
        pending.insert(
            SessionKey { id, generation: 7 },
            Pending {
                context: context(),
                action: Action::Ui(UiRequest::Filters(0)),
                phase: Phase::Queued,
            },
        );
    }
    let mut cursor = None;
    let mut served = Vec::new();
    for _ in 0..8 {
        let keys = select_pending(&pending, cursor, 1, |p| matches!(p, Phase::Queued));
        assert_eq!(keys.len(), 1);
        cursor = keys.first().copied();
        served.push(keys[0].id);
        // All actions stay queued, modeling recurring pre-authorization holds.
    }
    assert_eq!(served, [1, 2, 3, 4, 1, 2, 3, 4]);
    pending.get_mut(&key()).unwrap().phase = Phase::Submitted;
    assert_eq!(
        select_pending(&pending, cursor, 1, |p| matches!(p, Phase::Queued))[0].id,
        2
    );
}
