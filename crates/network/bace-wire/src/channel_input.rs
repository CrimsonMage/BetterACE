//! Typed decoders for the retail client channel-list (ListChannels 0x0148) and channel-index
//! (IndexChannels 0x0149) game actions, per ACE GameActionChannelList and GameActionChannelIndex.
//! Decode only: the channel id stays raw, and advocate/admin permission, channel policy and the
//! outbound GameEventChannelList/Index are domain or output concerns. ACE EnterPkLite (0x028F)
//! reads no payload, so it has no decoder here.
use crate::opcode::GameActionType as Op;
use crate::{Reader, WireError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelAction {
    ListChannels { channel: u32 },
    IndexChannels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelRequest {
    pub action: ChannelAction,
    /// Payload bytes not consumed by the ACE read sequence. ACE ignores them; they are reported.
    pub trailing_bytes: usize,
}

impl ChannelRequest {
    pub fn decode(opcode: Op, payload: &[u8], max_payload_bytes: usize) -> Result<Self, WireError> {
        if payload.len() > max_payload_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(payload);
        let action = match opcode {
            Op::ListChannels => ChannelAction::ListChannels {
                channel: reader.u32()?,
            },
            Op::IndexChannels => ChannelAction::IndexChannels,
            _ => return Err(WireError::UnexpectedOpcode(opcode.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: reader.remaining(),
        })
    }
}
