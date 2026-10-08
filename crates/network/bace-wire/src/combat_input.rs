//! Pinned ACE combat action readers. Values remain untrusted domain proposals.
use crate::opcode::GameActionType;
use crate::{Reader, WireError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatAction {
    TargetedMelee {
        target_id: u32,
        height: u32,
        power: f32,
    },
    TargetedMissile {
        target_id: u32,
        height: u32,
        accuracy: f32,
    },
    ChangeMode(u32),
    CancelAttack,
    QueryHealth(u32),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatRequest {
    pub action: CombatAction,
    /// Official handlers ignore suffix bytes; the input budget still bounds them.
    pub trailing_bytes: usize,
}
impl CombatRequest {
    pub fn decode(
        action: GameActionType,
        payload: &[u8],
        max_payload_bytes: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(payload);
        let action = match action {
            GameActionType::TargetedMeleeAttack => CombatAction::TargetedMelee {
                target_id: reader.u32()?,
                height: reader.u32()?,
                power: reader.f32()?,
            },
            GameActionType::TargetedMissileAttack => CombatAction::TargetedMissile {
                target_id: reader.u32()?,
                height: reader.u32()?,
                accuracy: reader.f32()?,
            },
            GameActionType::ChangeCombatMode => CombatAction::ChangeMode(reader.u32()?),
            GameActionType::CancelAttack => CombatAction::CancelAttack,
            GameActionType::QueryHealth => CombatAction::QueryHealth(reader.u32()?),
            _ => return Err(WireError::UnexpectedOpcode(action.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: reader.remaining(),
        })
    }
}
