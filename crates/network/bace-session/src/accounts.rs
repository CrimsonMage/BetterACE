use std::collections::BTreeMap;

use bace_types::AccountId;

use crate::SessionKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountSessionError {
    InvalidCapacity,
    Capacity,
    SessionAlreadyBound,
    ReplacementAlreadyPending,
    OwnerDraining,
    StaleSession,
    DrainNotRequested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountAdmission {
    Admitted,
    WaitingForDrain { owner: SessionKey },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountCancellation {
    PendingCancelled,
    OwnerDrainRequired,
}

struct AccountSlot {
    owner: SessionKey,
    pending: Option<SessionKey>,
    draining: bool,
}

/// One account owner and at most one pending replacement per account. Capacity
/// bounds account entries; the reverse index is bounded at twice that capacity.
/// The network registry must fence incoming asynchronous auth results BEFORE
/// calling request: this object does not retain an unbounded history of old keys.
/// Drain completion comes from world/persistence owners, never a client packet.
pub struct AccountSessions {
    accounts: BTreeMap<AccountId, AccountSlot>,
    sessions: BTreeMap<SessionKey, AccountId>,
    capacity: usize,
}

impl AccountSessions {
    pub fn new(capacity: usize) -> Result<Self, AccountSessionError> {
        if !(1..=4096).contains(&capacity) {
            return Err(AccountSessionError::InvalidCapacity);
        }
        Ok(Self {
            accounts: BTreeMap::new(),
            sessions: BTreeMap::new(),
            capacity,
        })
    }

    /// An authenticated replacement starts the old owner's drain and waits.
    /// The first pending replacement wins; a third login never replaces it.
    pub fn request(
        &mut self,
        account: AccountId,
        key: SessionKey,
    ) -> Result<AccountAdmission, AccountSessionError> {
        if self
            .sessions
            .get(&key)
            .is_some_and(|existing| *existing != account)
        {
            return Err(AccountSessionError::SessionAlreadyBound);
        }
        if let Some(slot) = self.accounts.get_mut(&account) {
            if slot.owner == key {
                return if slot.draining {
                    Err(AccountSessionError::OwnerDraining)
                } else {
                    Ok(AccountAdmission::Admitted)
                };
            }
            if slot.pending == Some(key) {
                return Ok(AccountAdmission::WaitingForDrain { owner: slot.owner });
            }
            if slot.pending.is_some() {
                return Err(AccountSessionError::ReplacementAlreadyPending);
            }
            slot.pending = Some(key);
            slot.draining = true;
            self.sessions.insert(key, account);
            return Ok(AccountAdmission::WaitingForDrain { owner: slot.owner });
        }
        if self.accounts.len() >= self.capacity {
            return Err(AccountSessionError::Capacity);
        }
        self.accounts.insert(
            account,
            AccountSlot {
                owner: key,
                pending: None,
                draining: false,
            },
        );
        self.sessions.insert(key, account);
        Ok(AccountAdmission::Admitted)
    }

    /// Cancellation/timeout of an owner never releases world ownership. It
    /// remains fenced until drained. A pending connection can be removed at once.
    pub fn cancel(
        &mut self,
        account: AccountId,
        key: SessionKey,
    ) -> Result<AccountCancellation, AccountSessionError> {
        let slot = self
            .accounts
            .get_mut(&account)
            .ok_or(AccountSessionError::StaleSession)?;
        if slot.owner == key {
            slot.draining = true;
            return Ok(AccountCancellation::OwnerDrainRequired);
        }
        if slot.pending == Some(key) {
            slot.pending = None;
            self.sessions.remove(&key);
            return Ok(AccountCancellation::PendingCancelled);
        }
        Err(AccountSessionError::StaleSession)
    }

    /// Complete the exact old owner's requested drain, promoting at most one
    /// waiting replacement. Repeated/stale completions cannot remove a new owner.
    pub fn drained(
        &mut self,
        account: AccountId,
        key: SessionKey,
    ) -> Result<Option<SessionKey>, AccountSessionError> {
        let slot = self
            .accounts
            .get_mut(&account)
            .ok_or(AccountSessionError::StaleSession)?;
        if slot.owner != key {
            return Err(AccountSessionError::StaleSession);
        }
        if !slot.draining {
            return Err(AccountSessionError::DrainNotRequested);
        }
        self.sessions.remove(&key);
        if let Some(next) = slot.pending.take() {
            slot.owner = next;
            slot.draining = false;
            Ok(Some(next))
        } else {
            self.accounts.remove(&account);
            Ok(None)
        }
    }

    pub fn owner(&self, account: AccountId) -> Option<SessionKey> {
        self.accounts.get(&account).map(|slot| slot.owner)
    }

    pub fn pending(&self, account: AccountId) -> Option<SessionKey> {
        self.accounts.get(&account).and_then(|slot| slot.pending)
    }

    pub fn account_for(&self, key: SessionKey) -> Option<AccountId> {
        self.sessions.get(&key).copied()
    }

    pub fn can_enter_world(&self, account: AccountId, key: SessionKey) -> bool {
        self.accounts
            .get(&account)
            .is_some_and(|slot| slot.owner == key && !slot.draining)
    }

    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}
