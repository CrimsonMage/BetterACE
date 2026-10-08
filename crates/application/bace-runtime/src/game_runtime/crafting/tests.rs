use super::*;
use bace_gameplay_api::SessionId;
use bace_types::{AccountId, EntityId};
fn runtime() -> CraftingRuntime {
    let mut runtime = CraftingRuntime::new();
    let context = ActionContext {
        actor: EntityId(0x50000001),
        account: AccountId(1),
        session: SessionId(7),
        sequence: 99,
    };
    runtime.pending = Some(Pending {
        key: SessionKey {
            id: 1,
            generation: 7,
        },
        context,
        binding: CharacterBinding {
            actor: context.actor,
            account: context.account,
            session: context.session,
        },
        source: 2,
        target: 3,
        operation: [3; 16],
        confirm: false,
        motion: None,
        phase: Phase::Submitted,
        submitted: Some(41),
        pending_operation: None,
        rejection: None,
        input: None,
        native: None,
        ticket: None,
        assets: None,
        completion: None,
    });
    runtime
}
#[test]
fn quote_and_durable_outcome_correlation_cannot_be_guessed_or_replayed() {
    let mut runtime = runtime();
    let wrong = CraftingOutcome {
        correlation: 40,
        result: Ok(CraftingResult::Pending(5)),
    };
    assert_eq!(*runtime.accept_owner(wrong.clone()).unwrap_err(), wrong);
    assert!(matches!(
        runtime.pending.as_ref().unwrap().phase,
        Phase::Submitted
    ));
    let right = CraftingOutcome {
        correlation: 41,
        result: Ok(CraftingResult::Pending(5)),
    };
    runtime.accept_owner(right.clone()).unwrap();
    let pending = runtime.pending.as_ref().unwrap();
    assert!(matches!(pending.phase, Phase::Saving));
    assert_eq!(pending.context.sequence, 99);
    assert_eq!(pending.operation, [3; 16]);
    assert_eq!(pending.pending_operation, Some(5));
    assert_eq!(*runtime.accept_owner(right.clone()).unwrap_err(), right);
    assert!(runtime.has_pending());
}
#[test]
fn consumed_owner_busy_is_reported_without_replaying_authenticated_sequence() {
    let mut runtime = runtime();
    runtime
        .accept_owner(CraftingOutcome {
            correlation: 41,
            result: Err(bace_crafting::CraftError::Busy),
        })
        .unwrap();
    assert!(matches!(
        runtime.pending.as_ref().unwrap().phase,
        Phase::Failed(_)
    ));
    assert_eq!(runtime.pending.as_ref().unwrap().context.sequence, 99);
}
#[test]
fn unrelated_error_capture_is_returned_without_losing_active_request() {
    let mut runtime = runtime();
    runtime.pending.as_mut().unwrap().phase = Phase::Capturing(3);
    let outcome = PlayerSnapshotOutcome {
        correlation: 4,
        result: Err(bace_simulation::CharacterRegistrationError::OwnershipMismatch),
    };
    assert!(runtime.accept_capture(outcome, 0).is_err());
    assert!(matches!(
        runtime.pending.as_ref().unwrap().phase,
        Phase::Capturing(3)
    ));
    assert!(runtime.has_pending());
}
