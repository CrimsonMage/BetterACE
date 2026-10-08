//! Pinned ACE GameActionMagicCastTargetedSpell/UntargetedSpell readers.
use crate::opcode::GameActionType;
use crate::{Reader, WireError};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagicAction {
    Targeted { target_id: u32, spell_id: u32 },
    Untargeted { spell_id: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MagicRequest {
    pub action: MagicAction,
    pub trailing_bytes: usize,
}
impl MagicRequest {
    pub fn decode(
        action: GameActionType,
        payload: &[u8],
        max_payload_bytes: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut r = Reader::new(payload);
        let action = match action {
            GameActionType::CastTargetedSpell => MagicAction::Targeted {
                target_id: r.u32()?,
                spell_id: r.u32()?,
            },
            GameActionType::CastUntargetedSpell => MagicAction::Untargeted { spell_id: r.u32()? },
            _ => return Err(WireError::UnexpectedOpcode(action.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: r.remaining(),
        })
    }
}
