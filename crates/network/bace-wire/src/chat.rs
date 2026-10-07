//! Pinned ACE UI messages. These contain wire values, never gameplay policy.
use crate::WireError;
use crate::envelope::message_writer;
use crate::opcode::GameMessageOpcode;

#[derive(Clone, Debug, PartialEq)]
pub enum ChatMessage<'a> {
    System {
        text: &'a str,
        chat_type: u32,
    },
    Speech {
        text: &'a str,
        sender_name: &'a str,
        sender_id: u32,
        chat_type: u32,
    },
    RangedSpeech {
        text: &'a str,
        sender_name: &'a str,
        sender_id: u32,
        range: f32,
        chat_type: u32,
    },
    Emote {
        sender_id: u32,
        sender_name: &'a str,
        text: &'a str,
    },
    SoulEmote {
        sender_id: u32,
        sender_name: &'a str,
        text: &'a str,
    },
}
impl ChatMessage<'_> {
    pub fn encode(&self) -> Result<Vec<u8>, WireError> {
        let opcode = match self {
            Self::System { .. } => GameMessageOpcode::ServerMessage,
            Self::Speech { .. } => GameMessageOpcode::HearSpeech,
            Self::RangedSpeech { .. } => GameMessageOpcode::HearRangedSpeech,
            Self::Emote { .. } => GameMessageOpcode::EmoteText,
            Self::SoulEmote { .. } => GameMessageOpcode::SoulEmote,
        };
        let mut writer = message_writer(opcode);
        match self {
            Self::System { text, chat_type } => {
                writer.string16(text)?;
                writer.u32(*chat_type);
            }
            Self::Speech {
                text,
                sender_name,
                sender_id,
                chat_type,
            }
            | Self::RangedSpeech {
                text,
                sender_name,
                sender_id,
                chat_type,
                ..
            } => {
                writer.string16(text)?;
                writer.string16(sender_name)?;
                writer.u32(*sender_id);
                if let Self::RangedSpeech { range, .. } = self {
                    writer.f32(*range);
                }
                writer.u32(*chat_type);
            }
            Self::Emote {
                sender_id,
                sender_name,
                text,
            }
            | Self::SoulEmote {
                sender_id,
                sender_name,
                text,
            } => {
                writer.u32(*sender_id);
                writer.string16(sender_name)?;
                writer.string16(text)?;
            }
        }
        Ok(writer.into_bytes())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerName<'a> {
    pub name: &'a str,
    pub current_connections: i32,
    /// ACE defaults to -1 for an unspecified capacity.
    pub max_connections: i32,
}
impl ServerName<'_> {
    pub fn encode(&self) -> Result<Vec<u8>, WireError> {
        let mut writer = message_writer(GameMessageOpcode::ServerName);
        writer.u32(self.current_connections as u32);
        writer.u32(self.max_connections as u32);
        writer.string16(self.name)?;
        Ok(writer.into_bytes())
    }
}
