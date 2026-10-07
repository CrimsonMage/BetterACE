//! Small ACE event payloads, including upstream's empty fellowship completion.
use crate::opcode::GameEventType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimpleGameEvent {
    WeenieError(u32),
    UseDone(u32),
    PingResponse,
    FellowshipFellowUpdateDone,
}
impl SimpleGameEvent {
    pub fn encode(self, object_id: u32, sequence: u32) -> Vec<u8> {
        let (event, value) = match self {
            Self::WeenieError(value) => (GameEventType::WeenieError, Some(value)),
            Self::UseDone(value) => (GameEventType::UseDone, Some(value)),
            Self::PingResponse => (GameEventType::PingResponse, None),
            Self::FellowshipFellowUpdateDone => (GameEventType::FellowshipFellowUpdateDone, None),
        };
        let payload = value.map(u32::to_le_bytes);
        // The payload is a fixed four-byte word or empty, always within this limit.
        let mut writer =
            crate::envelope::message_writer(crate::opcode::GameMessageOpcode::GameEvent);
        writer.u32(object_id);
        writer.u32(sequence);
        writer.u32(event.0);
        if let Some(payload) = payload {
            writer.bytes(&payload);
        }
        writer.into_bytes()
    }
}
