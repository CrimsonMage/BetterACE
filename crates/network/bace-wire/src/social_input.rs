//! Typed chat/friend/squelch requests. Strings and IDs remain untrusted; channel
//! permissions, name normalization, recipients and persistence are domain policy.
use crate::opcode::GameActionType as Op;
use crate::{Reader, WireError};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialAction {
    Talk(String),
    Tell {
        text: String,
        target_name: String,
    },
    TalkDirect {
        text: String,
        target_id: u32,
    },
    ChatChannel {
        channel: u32,
        text: String,
    },
    Emote(String),
    SoulEmote(String),
    SetAfkMode(bool),
    SetAfkMessage(String),
    AddFriend(String),
    RemoveFriend(u32),
    RemoveAllFriends,
    AddChannel(u32),
    RemoveChannel(u32),
    ModifyGlobalSquelch {
        enabled: bool,
        message_type: u32,
    },
    ModifyCharacterSquelch {
        enabled: bool,
        object_id: u32,
        name: String,
        message_type: u32,
    },
    ModifyAccountSquelch {
        enabled: bool,
        name: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocialRequest {
    pub action: SocialAction,
    pub trailing_bytes: usize,
}
impl SocialRequest {
    pub fn decode(
        opcode: Op,
        payload: &[u8],
        max_payload_bytes: usize,
        max_string_units: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(payload);
        let action = match opcode {
            Op::Talk => SocialAction::Talk(reader.client_string16(max_string_units)?),
            Op::Tell => SocialAction::Tell {
                text: reader.client_string16(max_string_units)?,
                target_name: reader.client_string16(max_string_units)?,
            },
            Op::TalkDirect => SocialAction::TalkDirect {
                text: reader.client_string16(max_string_units)?,
                target_id: reader.u32()?,
            },
            Op::ChatChannel => SocialAction::ChatChannel {
                channel: reader.u32()?,
                text: reader.client_string16(max_string_units)?,
            },
            Op::Emote => SocialAction::Emote(reader.client_string16(max_string_units)?),
            Op::SoulEmote => SocialAction::SoulEmote(reader.client_string16(max_string_units)?),
            Op::SetAfkMode => SocialAction::SetAfkMode(reader.u32()? != 0),
            Op::SetAfkMessage => {
                SocialAction::SetAfkMessage(reader.client_string16(max_string_units)?)
            }
            Op::AddFriend => SocialAction::AddFriend(reader.client_string16(max_string_units)?),
            Op::RemoveFriend => SocialAction::RemoveFriend(reader.u32()?),
            Op::RemoveAllFriends => SocialAction::RemoveAllFriends,
            Op::AddChannel => SocialAction::AddChannel(reader.u32()?),
            Op::RemoveChannel => SocialAction::RemoveChannel(reader.u32()?),
            Op::ModifyGlobalSquelch => SocialAction::ModifyGlobalSquelch {
                enabled: reader.u32()? != 0,
                message_type: reader.u32()?,
            },
            Op::ModifyCharacterSquelch => SocialAction::ModifyCharacterSquelch {
                enabled: reader.u32()? != 0,
                object_id: reader.u32()?,
                name: reader.client_string16(max_string_units)?,
                message_type: reader.u32()?,
            },
            Op::ModifyAccountSquelch => SocialAction::ModifyAccountSquelch {
                enabled: reader.u32()? != 0,
                name: reader.client_string16(max_string_units)?,
            },
            _ => return Err(WireError::UnexpectedOpcode(opcode.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: reader.remaining(),
        })
    }
}
