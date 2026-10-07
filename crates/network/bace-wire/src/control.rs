//! Pinned ACE account control messages; elapsed time is an explicit input.
use crate::WireError;
use crate::envelope::message_writer;
use crate::opcode::GameMessageOpcode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountControl<'a> {
    /// `None` emits the opcode only; `Some("")` emits an empty String16L.
    Boot { reason: Option<&'a str> },
    Banned {
        seconds_remaining: u32,
        reason: Option<&'a str>,
    },
}
impl AccountControl<'_> {
    pub fn encode(&self) -> Result<Vec<u8>, WireError> {
        let (opcode, reason) = match self {
            Self::Boot { reason } => (GameMessageOpcode::AccountBoot, reason),
            Self::Banned { reason, .. } => (GameMessageOpcode::AccountBanned, reason),
        };
        let mut writer = message_writer(opcode);
        if let Self::Banned {
            seconds_remaining, ..
        } = self
        {
            writer.u32(*seconds_remaining);
        }
        if let Some(reason) = reason {
            writer.string16(reason)?;
        }
        Ok(writer.into_bytes())
    }
}
