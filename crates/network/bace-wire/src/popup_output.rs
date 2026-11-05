//! Outbound ACE `GameEventPopupString` (GameEvent 0x0004). Official source:
//! `Source/ACE.Server/Network/GameEvent/Events/GameEventPopupString.cs` at
//! 47edade3bd3f6044b676d4eb877c4965c7eda62b. The base header is guid, session
//! GameEventSequence, event type; then `WriteString16L`.
//!
//! Deviation (hardening): ACE substitutes `?` for characters outside
//! Windows-1252 and silently truncates the u16 length of strings longer than
//! 65535 characters. This encoder rejects both cases instead, so no text is
//! altered or truncated without an error.
use crate::WireError;
use crate::envelope::message_writer;
use crate::opcode::{GameEventType, GameMessageOpcode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopupString<'a> {
    pub message: &'a str,
}
impl PopupString<'_> {
    /// `object_id` is the player guid (0 when no player is attached) and
    /// `sequence` is the session's GameEventSequence value for this message.
    pub fn encode(self, object_id: u32, sequence: u32) -> Result<Vec<u8>, WireError> {
        let mut writer = message_writer(GameMessageOpcode::GameEvent);
        writer.u32(object_id);
        writer.u32(sequence);
        writer.u32(GameEventType::PopupString.0);
        writer.string16(self.message)?;
        Ok(writer.into_bytes())
    }
}
