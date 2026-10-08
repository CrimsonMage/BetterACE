//! Pinned ACE recall handlers take no payload fields. A bounded suffix is ignored
//! and reported, never interpreted as a destination or permission claim.
use crate::{WireError, opcode::GameActionType as Action};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecallAction {
    Lifestone,
    House,
    Marketplace,
    AllegianceHometown,
    AllegianceHousing,
    PkArena,
    PklArena,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecallRequest {
    pub action: RecallAction,
    pub trailing_bytes: usize,
}
impl RecallRequest {
    pub fn decode(opcode: Action, payload: &[u8], maximum: usize) -> Result<Self, WireError> {
        if payload.len() > maximum {
            return Err(WireError::LimitExceeded);
        }
        let action = match opcode {
            Action::TeleToLifestone => RecallAction::Lifestone,
            Action::TeleToHouse => RecallAction::House,
            Action::TeleToMarketPlace => RecallAction::Marketplace,
            Action::RecallAllegianceHometown => RecallAction::AllegianceHometown,
            Action::TeleToMansion => RecallAction::AllegianceHousing,
            Action::TeleToPkArena => RecallAction::PkArena,
            Action::TeleToPklArena => RecallAction::PklArena,
            other => return Err(WireError::UnexpectedOpcode(other.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: payload.len(),
        })
    }
}
