//! ACE GameActionRaise{Attribute,Vital,Skill} and GameActionTrainSkill handlers.
//! Requested amounts are untrusted; this module grants no progression or credits.
use crate::opcode::GameActionType;
use crate::{Reader, WireError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressionAction {
    RaiseAttribute {
        attribute: u32,
        experience_spent: u32,
    },
    RaiseVital {
        vital: u32,
        experience_spent: u32,
    },
    RaiseSkill {
        skill: u32,
        experience_spent: u32,
    },
    TrainSkill {
        skill: u32,
        credits_spent: i32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgressionRequest {
    pub action: ProgressionAction,
    /// All four official handlers consume eight bytes and ignore any suffix.
    pub trailing_bytes: usize,
}
impl ProgressionRequest {
    pub fn decode(
        action: GameActionType,
        payload: &[u8],
        max_payload_bytes: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload_bytes {
            return Err(WireError::LimitExceeded);
        }
        if !matches!(
            action,
            GameActionType::RaiseAttribute
                | GameActionType::RaiseVital
                | GameActionType::RaiseSkill
                | GameActionType::TrainSkill
        ) {
            return Err(WireError::UnexpectedOpcode(action.0));
        }
        let mut reader = Reader::new(payload);
        let target = reader.u32()?;
        let amount = reader.u32()?;
        // Upstream casts to ushort-backed attribute enums. Reject high bits
        // instead of allowing an invalid target to alias a valid attribute.
        if matches!(
            action,
            GameActionType::RaiseAttribute | GameActionType::RaiseVital
        ) && target > u32::from(u16::MAX)
        {
            return Err(WireError::InvalidEncoding);
        }
        let action = match action {
            GameActionType::RaiseAttribute => ProgressionAction::RaiseAttribute {
                attribute: target,
                experience_spent: amount,
            },
            GameActionType::RaiseVital => ProgressionAction::RaiseVital {
                vital: target,
                experience_spent: amount,
            },
            GameActionType::RaiseSkill => ProgressionAction::RaiseSkill {
                skill: target,
                experience_spent: amount,
            },
            GameActionType::TrainSkill => ProgressionAction::TrainSkill {
                skill: target,
                credits_spent: amount as i32,
            },
            _ => return Err(WireError::UnexpectedOpcode(action.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: reader.remaining(),
        })
    }
}
