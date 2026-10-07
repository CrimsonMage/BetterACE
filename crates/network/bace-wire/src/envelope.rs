//! ACE GameMessage, GameActionPacket and GameEventMessage framing. Payload
//! interpretation belongs to a separate codec; recognizing an ID is not support.
use crate::opcode::{GameActionType, GameEventType, GameMessageOpcode};
use crate::{Reader, WireError, Writer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameActionEnvelope<'a> {
    pub sequence: u32,
    pub action: GameActionType,
    pub payload: &'a [u8],
}
impl<'a> GameActionEnvelope<'a> {
    pub fn decode(bytes: &'a [u8], max_payload_bytes: usize) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        expect_opcode(&mut reader, GameMessageOpcode::GameAction)?;
        let sequence = reader.u32()?;
        let action = GameActionType(reader.u32()?);
        let payload = remaining(&mut reader, max_payload_bytes)?;
        Ok(Self {
            sequence,
            action,
            payload,
        })
    }
    pub fn encode(&self, max_payload_bytes: usize) -> Result<Vec<u8>, WireError> {
        check_limit(self.payload, max_payload_bytes)?;
        let mut writer = message_writer(GameMessageOpcode::GameAction);
        writer.u32(self.sequence);
        writer.u32(self.action.0);
        writer.bytes(self.payload);
        Ok(writer.into_bytes())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameEventEnvelope<'a> {
    pub object_id: u32,
    pub sequence: u32,
    pub event: GameEventType,
    pub payload: &'a [u8],
}
impl<'a> GameEventEnvelope<'a> {
    pub fn decode(bytes: &'a [u8], max_payload_bytes: usize) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        expect_opcode(&mut reader, GameMessageOpcode::GameEvent)?;
        let object_id = reader.u32()?;
        let sequence = reader.u32()?;
        let event = GameEventType(reader.u32()?);
        let payload = remaining(&mut reader, max_payload_bytes)?;
        Ok(Self {
            object_id,
            sequence,
            event,
            payload,
        })
    }
    pub fn encode(&self, max_payload_bytes: usize) -> Result<Vec<u8>, WireError> {
        check_limit(self.payload, max_payload_bytes)?;
        let mut writer = message_writer(GameMessageOpcode::GameEvent);
        writer.u32(self.object_id);
        writer.u32(self.sequence);
        writer.u32(self.event.0);
        writer.bytes(self.payload);
        Ok(writer.into_bytes())
    }
}
fn check_limit(payload: &[u8], limit: usize) -> Result<(), WireError> {
    if payload.len() > limit {
        Err(WireError::LimitExceeded)
    } else {
        Ok(())
    }
}
fn remaining<'a>(reader: &mut Reader<'a>, limit: usize) -> Result<&'a [u8], WireError> {
    if reader.remaining() > limit {
        return Err(WireError::LimitExceeded);
    }
    reader.take(reader.remaining())
}
pub(crate) fn message_writer(opcode: GameMessageOpcode) -> Writer {
    let mut writer = Writer::new();
    writer.u32(opcode.0);
    writer
}
pub(crate) fn expect_opcode(
    reader: &mut Reader<'_>,
    opcode: GameMessageOpcode,
) -> Result<(), WireError> {
    let actual = reader.u32()?;
    if actual != opcode.0 {
        return Err(WireError::UnexpectedOpcode(actual));
    }
    Ok(())
}
pub(crate) fn finish(reader: &Reader<'_>) -> Result<(), WireError> {
    if reader.remaining() != 0 {
        return Err(WireError::InvalidLength);
    }
    Ok(())
}
