//! Generation-fenced character admission state. No gameplay or physical state lives here.
use bace_auth::AccountName;
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_session::SessionKey;
use bace_types::{AccountId, EntityId};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameLoginPhase {
    Roster,
    Creating,
    Loading,
    AwaitingWorld,
    Admitting,
    Online,
    Aborting,
}
#[derive(Clone, Debug)]
pub(crate) enum AdmissionState {
    Roster,
    Creating {
        character_id: u32,
        fingerprint: [u8; 32],
        player_fingerprint: [u8; 32],
    },
    Loading {
        previous: CharacterLease,
    },
    AwaitingWorld {
        lease: CharacterLease,
    },
    Admitting {
        lease: CharacterLease,
    },
    Online {
        lease: CharacterLease,
    },
    Aborting {
        lease: CharacterLease,
    },
}
impl AdmissionState {
    pub(crate) fn phase(&self) -> GameLoginPhase {
        match self {
            Self::Roster => GameLoginPhase::Roster,
            Self::Creating { .. } => GameLoginPhase::Creating,
            Self::Loading { .. } => GameLoginPhase::Loading,
            Self::AwaitingWorld { .. } => GameLoginPhase::AwaitingWorld,
            Self::Admitting { .. } => GameLoginPhase::Admitting,
            Self::Online { .. } => GameLoginPhase::Online,
            Self::Aborting { .. } => GameLoginPhase::Aborting,
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct AccountSession {
    pub account: AccountId,
    pub name: AccountName,
    pub state: AdmissionState,
}
/// Bounded lifecycle metadata only. Removing a pending/online session is forbidden;
/// termination must resolve its operation and transfer/drain the single world owner.
pub(crate) struct AdmissionRegistry {
    sessions: BTreeMap<SessionKey, AccountSession>,
    capacity: usize,
}
impl AdmissionRegistry {
    pub(crate) fn new(capacity: usize) -> Result<Self, super::game_login::GameLoginError> {
        if !(1..=4096).contains(&capacity) {
            return Err(super::game_login::GameLoginError::Capacity);
        }
        Ok(Self {
            sessions: BTreeMap::new(),
            capacity,
        })
    }
    pub(crate) fn register(
        &mut self,
        key: SessionKey,
        account: AccountId,
        name: AccountName,
    ) -> Result<(), super::game_login::GameLoginError> {
        use super::game_login::GameLoginError as Error;
        if key.generation == 0 || account.0 == 0 {
            return Err(Error::InvalidIdentity);
        }
        if self.sessions.len() >= self.capacity {
            return Err(Error::Capacity);
        }
        if self.sessions.keys().any(|known| known.id == key.id)
            || self.sessions.values().any(|known| known.account == account)
        {
            return Err(Error::DrainRequired);
        }
        self.sessions.insert(
            key,
            AccountSession {
                account,
                name,
                state: AdmissionState::Roster,
            },
        );
        Ok(())
    }
    pub(crate) fn get(
        &self,
        key: SessionKey,
    ) -> Result<&AccountSession, super::game_login::GameLoginError> {
        self.sessions
            .get(&key)
            .ok_or(super::game_login::GameLoginError::StaleSession)
    }
    pub(crate) fn set(
        &mut self,
        key: SessionKey,
        state: AdmissionState,
    ) -> Result<(), super::game_login::GameLoginError> {
        self.sessions
            .get_mut(&key)
            .ok_or(super::game_login::GameLoginError::StaleSession)?
            .state = state;
        Ok(())
    }
    pub(crate) fn retire(
        &mut self,
        key: SessionKey,
    ) -> Result<(), super::game_login::GameLoginError> {
        if !matches!(self.get(key)?.state, AdmissionState::Roster) {
            return Err(super::game_login::GameLoginError::DrainRequired);
        }
        self.sessions.remove(&key);
        Ok(())
    }
}
pub(crate) fn binding(key: SessionKey, account: AccountId, character_id: u32) -> CharacterBinding {
    CharacterBinding {
        session: SessionId(key.generation),
        account,
        actor: EntityId(character_id),
    }
}
pub(crate) fn expected_loading(previous: CharacterLease) -> Option<CharacterLease> {
    Some(CharacterLease {
        epoch: previous.epoch.checked_add(1)?,
        state: OwnershipState::Loading,
        ..previous
    })
}
