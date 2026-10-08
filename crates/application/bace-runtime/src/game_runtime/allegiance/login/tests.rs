use super::*;
use bace_gameplay_api::SessionId;
use bace_types::{AccountId, EntityId};

fn session(generation: u64) -> (SessionKey, CharacterBinding) {
    (
        SessionKey { id: 1, generation },
        CharacterBinding {
            actor: EntityId(0x50000001),
            account: AccountId(7),
            session: SessionId(generation),
        },
    )
}

#[test]
fn prepared_redemption_waits_for_exact_durable_completion_and_retries_rejection() {
    let (key, binding) = session(9);
    let entered = [(key, binding)];
    let mut state = LoginRedemptions::default();
    assert_eq!(state.candidate(&entered), Some((key, binding)));
    state.start(31, key, binding).unwrap();
    assert!(state.has_pending());
    assert!(state.candidate(&entered).is_none());
    assert!(
        state
            .accept(SocialControlOutcome {
                sequence: 32,
                result: Ok(Some(91)),
            })
            .is_err()
    );
    assert!(state.owns(31));
    assert_eq!(
        state
            .accept(SocialControlOutcome {
                sequence: 31,
                result: Ok(Some(91)),
            })
            .unwrap(),
        (key, binding, Ok(Some(91)))
    );
    assert!(state.candidate(&entered).is_none());
    assert!(!state.finish(92, true));
    assert!(state.candidate(&entered).is_none());
    assert!(state.finish(91, false));
    assert_eq!(state.candidate(&entered), Some((key, binding)));
    state.start(33, key, binding).unwrap();
    assert_eq!(
        state
            .accept(SocialControlOutcome {
                sequence: 33,
                result: Ok(Some(93)),
            })
            .unwrap(),
        (key, binding, Ok(Some(93)))
    );
    assert!(state.finish(93, true));
    assert!(state.candidate(&entered).is_none());
    assert!(!state.has_pending());
}

#[test]
fn busy_retry_and_session_generation_are_independent() {
    let (first_key, first_binding) = session(9);
    let (next_key, next_binding) = session(10);
    let mut state = LoginRedemptions::default();
    state.start(41, first_key, first_binding).unwrap();
    assert_eq!(
        state
            .accept(SocialControlOutcome {
                sequence: 41,
                result: Err(SocialError::Busy),
            })
            .unwrap(),
        (first_key, first_binding, Err(SocialError::Busy))
    );
    assert_eq!(
        state.candidate(&[(first_key, first_binding)]),
        Some((first_key, first_binding))
    );
    state.start(42, first_key, first_binding).unwrap();
    assert_eq!(
        state
            .accept(SocialControlOutcome {
                sequence: 42,
                result: Ok(None),
            })
            .unwrap(),
        (first_key, first_binding, Ok(None))
    );
    assert!(state.candidate(&[(first_key, first_binding)]).is_none());
    state.retain_entered(&[(next_key, next_binding)]);
    assert_eq!(
        state.candidate(&[(next_key, next_binding)]),
        Some((next_key, next_binding))
    );
}
