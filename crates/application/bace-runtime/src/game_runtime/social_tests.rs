use super::*;
use bace_gameplay_api::SessionId;
use bace_types::{AccountId, EntityId};
fn context(sequence: u32) -> ActionContext {
    ActionContext {
        actor: EntityId(0x50000001),
        account: AccountId(7),
        session: SessionId(9),
        sequence,
    }
}
fn pending() -> (SessionKey, Pending) {
    let key = SessionKey {
        id: 1,
        generation: 9,
    };
    let command = Command::SocialResolved {
        context: context(22),
        request: SocialRequest::AccountSquelch {
            enabled: true,
            name: "Offline".into(),
        },
        identity: Some(SocialIdentity {
            character: EntityId(0x50000002),
            account: AccountId(88),
            name: "Offline".into(),
        }),
    };
    let mut pending = Pending::from_command(&command).unwrap();
    pending.submitted = true;
    (key, pending)
}
#[test]
fn valuable_hold_retries_exact_verified_action_and_removes_barrier_only_after_receipt() {
    let (key, action) = pending();
    let mut pending = BTreeMap::from([(key, action)]);
    let mut failures = BTreeMap::new();
    correlate_outcome(
        &mut pending,
        &mut failures,
        SocialOutcome {
            retryable: true,
            context: context(22),
            result: Err(SocialError::Busy),
        },
    )
    .unwrap();
    assert!(!pending[&key].submitted);
    assert!(
        matches!(pending[&key].command(),Command::SocialResolved{context:c,identity:Some(identity),request:SocialRequest::AccountSquelch{enabled:true,name}} if c==context(22)&&identity.account==AccountId(88)&&name=="Offline")
    );
    // A second completion before the exact retry was submitted cannot clear it.
    let premature = SocialOutcome {
        retryable: false,
        context: context(22),
        result: Ok(()),
    };
    assert_eq!(
        *correlate_outcome(&mut pending, &mut failures, premature.clone()).unwrap_err(),
        premature
    );
    assert_eq!(pending.len(), 1);
    pending.get_mut(&key).unwrap().submitted = true;
    correlate_outcome(
        &mut pending,
        &mut failures,
        SocialOutcome {
            retryable: false,
            context: context(22),
            result: Ok(()),
        },
    )
    .unwrap();
    assert!(pending.is_empty());
    assert!(failures.is_empty());
}
#[test]
fn stale_generation_or_sequence_receipts_preserve_original_pending_command() {
    for stale in [
        ActionContext {
            session: SessionId(10),
            ..context(22)
        },
        context(23),
    ] {
        let (key, action) = pending();
        let mut pending = BTreeMap::from([(key, action)]);
        let mut failures = BTreeMap::new();
        let outcome = SocialOutcome {
            retryable: false,
            context: stale,
            result: Ok(()),
        };
        assert_eq!(
            *correlate_outcome(&mut pending, &mut failures, outcome.clone()).unwrap_err(),
            outcome
        );
        assert!(pending[&key].submitted);
        assert_eq!(pending[&key].context, context(22));
        correlate_outcome(
            &mut pending,
            &mut failures,
            SocialOutcome {
                retryable: false,
                context: context(22),
                result: Err(SocialError::Forbidden),
            },
        )
        .unwrap();
        assert!(pending.is_empty());
        assert_eq!(failures[&key], SocialError::Forbidden);
    }
}
#[test]
fn post_authorization_busy_is_a_consumed_rejection_not_a_replay() {
    let (key, action) = pending();
    let mut pending = BTreeMap::from([(key, action)]);
    let mut failures = BTreeMap::new();
    correlate_outcome(
        &mut pending,
        &mut failures,
        SocialOutcome {
            context: context(22),
            result: Err(SocialError::Busy),
            retryable: false,
        },
    )
    .unwrap();
    assert!(pending.is_empty());
    assert_eq!(failures[&key], SocialError::Busy);
}
