use bace_session::{
    AccountAdmission, AccountCancellation, AccountSessionError, AccountSessions, SessionKey,
};
use bace_types::AccountId;

fn key(id: u16, generation: u64) -> SessionKey {
    SessionKey { id, generation }
}

#[test]
fn replacement_waits_for_exact_owner_drain_and_stale_completion_is_harmless() {
    let mut accounts = AccountSessions::new(2).unwrap();
    let account = AccountId(1);
    let old = key(0, 1);
    let new = key(1, 2);
    assert_eq!(
        accounts.request(account, old),
        Ok(AccountAdmission::Admitted)
    );
    assert!(accounts.can_enter_world(account, old));
    assert_eq!(
        accounts.request(account, new),
        Ok(AccountAdmission::WaitingForDrain { owner: old })
    );
    assert_eq!(accounts.owner(account), Some(old));
    assert!(!accounts.can_enter_world(account, old));
    assert!(!accounts.can_enter_world(account, new));
    assert_eq!(
        accounts.drained(account, new),
        Err(AccountSessionError::StaleSession)
    );
    assert_eq!(accounts.drained(account, old), Ok(Some(new)));
    assert_eq!(
        accounts.drained(account, old),
        Err(AccountSessionError::StaleSession)
    );
    assert!(accounts.can_enter_world(account, new));
    assert_eq!(accounts.session_count(), 1);
    assert_eq!(accounts.account_for(old), None);
}

#[test]
fn one_pending_replacement_is_stable_and_duplicate_requests_are_idempotent() {
    let mut accounts = AccountSessions::new(1).unwrap();
    let account = AccountId(1);
    let old = key(0, 1);
    let pending = key(1, 2);
    accounts.request(account, old).unwrap();
    assert_eq!(
        accounts.request(account, old),
        Ok(AccountAdmission::Admitted)
    );
    accounts.request(account, pending).unwrap();
    assert_eq!(
        accounts.request(account, pending),
        Ok(AccountAdmission::WaitingForDrain { owner: old })
    );
    assert_eq!(
        accounts.request(account, key(2, 3)),
        Err(AccountSessionError::ReplacementAlreadyPending)
    );
    assert_eq!(
        accounts.request(account, old),
        Err(AccountSessionError::OwnerDraining)
    );
    assert_eq!(accounts.pending(account), Some(pending));
    assert_eq!(accounts.session_count(), 2);
    assert_eq!(accounts.account_for(key(2, 3)), None);
}

#[test]
fn pending_timeout_cannot_cancel_old_owners_required_drain() {
    let mut accounts = AccountSessions::new(1).unwrap();
    let account = AccountId(1);
    let old = key(0, 1);
    let pending = key(1, 2);
    accounts.request(account, old).unwrap();
    accounts.request(account, pending).unwrap();
    assert_eq!(
        accounts.cancel(account, pending),
        Ok(AccountCancellation::PendingCancelled)
    );
    assert!(!accounts.can_enter_world(account, old));
    assert_eq!(accounts.pending(account), None);
    assert_eq!(accounts.account_for(pending), None);
    assert_eq!(accounts.drained(account, old), Ok(None));
    assert!(accounts.is_empty());
    assert_eq!(accounts.session_count(), 0);
}

#[test]
fn owner_timeout_retains_capacity_until_durable_drain_confirmation() {
    let mut accounts = AccountSessions::new(1).unwrap();
    let old = key(0, 1);
    accounts.request(AccountId(1), old).unwrap();
    assert_eq!(
        accounts.drained(AccountId(1), old),
        Err(AccountSessionError::DrainNotRequested)
    );
    assert_eq!(
        accounts.cancel(AccountId(1), old),
        Ok(AccountCancellation::OwnerDrainRequired)
    );
    assert_eq!(
        accounts.request(AccountId(2), key(1, 2)),
        Err(AccountSessionError::Capacity)
    );
    assert_eq!(accounts.owner(AccountId(1)), Some(old));
    assert_eq!(accounts.drained(AccountId(1), old), Ok(None));
    assert_eq!(
        accounts.request(AccountId(2), key(1, 2)),
        Ok(AccountAdmission::Admitted)
    );
}

#[test]
fn cancelled_pending_can_be_replaced_without_losing_original_drain_identity() {
    let mut accounts = AccountSessions::new(1).unwrap();
    let account = AccountId(1);
    let old = key(0, 1);
    let first = key(1, 2);
    let second = key(2, 3);
    accounts.request(account, old).unwrap();
    accounts.request(account, first).unwrap();
    accounts.cancel(account, first).unwrap();
    assert_eq!(
        accounts.request(account, second),
        Ok(AccountAdmission::WaitingForDrain { owner: old })
    );
    assert_eq!(accounts.drained(account, old), Ok(Some(second)));
    assert_eq!(
        accounts.cancel(account, first),
        Err(AccountSessionError::StaleSession)
    );
    assert!(accounts.can_enter_world(account, second));
}

#[test]
fn session_generations_cannot_be_shared_between_accounts() {
    let mut accounts = AccountSessions::new(2).unwrap();
    let old = key(0, 1);
    let next_generation = key(0, 2);
    accounts.request(AccountId(1), old).unwrap();
    assert_eq!(
        accounts.request(AccountId(2), old),
        Err(AccountSessionError::SessionAlreadyBound)
    );
    accounts.request(AccountId(1), next_generation).unwrap();
    assert_eq!(
        accounts.request(AccountId(2), next_generation),
        Err(AccountSessionError::SessionAlreadyBound)
    );
    accounts.drained(AccountId(1), old).unwrap();
    assert_eq!(
        accounts.cancel(AccountId(1), old),
        Err(AccountSessionError::StaleSession)
    );
    assert_eq!(
        accounts.drained(AccountId(2), next_generation),
        Err(AccountSessionError::StaleSession)
    );
    assert!(accounts.can_enter_world(AccountId(1), next_generation));
}

#[test]
fn all_states_stay_within_global_bounds() {
    assert!(matches!(
        AccountSessions::new(0),
        Err(AccountSessionError::InvalidCapacity)
    ));
    assert!(matches!(
        AccountSessions::new(4097),
        Err(AccountSessionError::InvalidCapacity)
    ));
    let mut accounts = AccountSessions::new(16).unwrap();
    for id in 0..16u16 {
        accounts
            .request(AccountId(u64::from(id)), key(id, u64::from(id) + 1))
            .unwrap();
        accounts
            .request(AccountId(u64::from(id)), key(id + 16, u64::from(id) + 17))
            .unwrap();
    }
    assert_eq!(accounts.len(), 16);
    assert_eq!(accounts.session_count(), 32);
    assert_eq!(
        accounts.request(AccountId(99), key(99, 99)),
        Err(AccountSessionError::Capacity)
    );
    for id in 0..16u16 {
        assert_eq!(
            accounts.drained(AccountId(u64::from(id)), key(id, u64::from(id) + 1)),
            Ok(Some(key(id + 16, u64::from(id) + 17)))
        );
    }
    assert_eq!(accounts.len(), 16);
    assert_eq!(accounts.session_count(), 16);
}
