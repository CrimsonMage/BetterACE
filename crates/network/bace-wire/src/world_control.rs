//! World control readers. LoginComplete observes portal exit, never authorizes entry.
use crate::opcode::{GameActionType, GameMessageOpcode};
use crate::{GameActionEnvelope, Reader, WireError};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldControlAction {
    LoginComplete,
    ForceObjectDescription(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldControlRequest {
    pub action: WorldControlAction,
    pub action_sequence: Option<u32>,
    pub trailing_bytes: usize,
}
impl WorldControlRequest {
    pub fn decode(bytes: &[u8], max_message_bytes: usize) -> Result<Self, WireError> {
        if bytes.len() > max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        match GameMessageOpcode(reader.u32()?) {
            GameMessageOpcode::GameAction => {
                let envelope = GameActionEnvelope::decode(bytes, max_message_bytes)?;
                if envelope.action != GameActionType::LoginComplete {
                    return Err(WireError::UnexpectedOpcode(envelope.action.0));
                }
                Ok(Self {
                    action: WorldControlAction::LoginComplete,
                    action_sequence: Some(envelope.sequence),
                    trailing_bytes: envelope.payload.len(),
                })
            }
            GameMessageOpcode::ForceObjectDescSend => Ok(Self {
                action: WorldControlAction::ForceObjectDescription(reader.u32()?),
                action_sequence: None,
                trailing_bytes: reader.remaining(),
            }),
            other => Err(WireError::UnexpectedOpcode(other.0)),
        }
    }
}
