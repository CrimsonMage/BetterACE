//! CharacterHandler input prefixes. Ownership checks remain server-side policy.
use crate::opcode::GameMessageOpcode as Message;
use crate::{Reader, WireError};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CharacterLifecycleAction {
    EnterWorldRequest,
    EnterWorld { character_id: u32, account: String },
    LogOff,
    Delete { account: String, slot: u32 },
    Restore { character_id: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterLifecycleRequest {
    pub action: CharacterLifecycleAction,
    pub trailing_bytes: usize,
}
impl CharacterLifecycleRequest {
    pub fn decode(
        bytes: &[u8],
        max_message_bytes: usize,
        max_string_units: usize,
    ) -> Result<Self, WireError> {
        if bytes.len() > max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        let opcode = Message(reader.u32()?);
        let action = match opcode {
            Message::CharacterEnterWorldRequest => CharacterLifecycleAction::EnterWorldRequest,
            Message::CharacterEnterWorld => CharacterLifecycleAction::EnterWorld {
                character_id: reader.u32()?,
                account: reader.client_string16(max_string_units)?,
            },
            Message::CharacterLogOff => CharacterLifecycleAction::LogOff,
            Message::CharacterDelete => CharacterLifecycleAction::Delete {
                account: reader.client_string16(max_string_units)?,
                slot: reader.u32()?,
            },
            Message::CharacterRestore => CharacterLifecycleAction::Restore {
                character_id: reader.u32()?,
            },
            _ => return Err(WireError::UnexpectedOpcode(opcode.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: reader.remaining(),
        })
    }
}
