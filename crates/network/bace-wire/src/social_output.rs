//! ACE social event layouts. All names, visibility and access-filter decisions
//! arrive as explicit immutable projections from the owning gameplay subsystem.
use crate::envelope::message_writer;
use crate::opcode::{GameEventType, GameMessageOpcode};
use crate::social_tables::string;
use crate::{FriendsUpdate, SquelchDatabase, WireError, Writer};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SocialCodecLimits {
    pub max_message_bytes: usize,
    pub max_entries: usize,
    pub max_filters: usize,
    pub max_string_bytes: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SocialEvent<'a> {
    Tell {
        text: &'a str,
        sender_name: &'a str,
        sender_id: u32,
        target_id: u32,
        chat_type: u32,
    },
    ChannelBroadcast {
        channel: u32,
        sender_name: &'a str,
        text: &'a str,
    },
    Transient(&'a str),
    TurbineChannels {
        allegiance: u32,
        society: u32,
    },
    Friends(&'a FriendsUpdate),
    Squelch(&'a SquelchDatabase),
    ChannelList(&'a [String]),
    /// Original ChannelIndex can emit zero, one or several privilege lists.
    /// Projection supplies those lists in the source's admin/sentinel/advocate order.
    ChannelIndex(&'a [Vec<String>]),
}
impl SocialEvent<'_> {
    pub fn encode(
        &self,
        object_id: u32,
        sequence: u32,
        limits: SocialCodecLimits,
    ) -> Result<Vec<u8>, WireError> {
        let event = match self {
            Self::Tell { .. } => GameEventType::Tell,
            Self::ChannelBroadcast { .. } => GameEventType::ChannelBroadcast,
            Self::Transient(_) => GameEventType::CommunicationTransientString,
            Self::TurbineChannels { .. } => GameEventType::SetTurbineChatChannels,
            Self::Friends(_) => GameEventType::FriendsListUpdate,
            Self::Squelch(_) => GameEventType::SetSquelchDB,
            Self::ChannelList(_) => GameEventType::ChannelList,
            Self::ChannelIndex(_) => GameEventType::ChannelIndex,
        };
        let mut writer = message_writer(GameMessageOpcode::GameEvent);
        writer.u32(object_id);
        writer.u32(sequence);
        writer.u32(event.0);
        match self {
            Self::Tell {
                text,
                sender_name,
                sender_id,
                target_id,
                chat_type,
            } => {
                string(&mut writer, text, limits.max_string_bytes)?;
                string(&mut writer, sender_name, limits.max_string_bytes)?;
                for value in [*sender_id, *target_id, *chat_type, 0] {
                    writer.u32(value);
                }
            }
            Self::ChannelBroadcast {
                channel,
                sender_name,
                text,
            } => {
                writer.u32(*channel);
                string(&mut writer, sender_name, limits.max_string_bytes)?;
                string(&mut writer, text, limits.max_string_bytes)?;
            }
            Self::Transient(text) => string(&mut writer, text, limits.max_string_bytes)?,
            Self::TurbineChannels {
                allegiance,
                society,
            } => {
                for value in [*allegiance, 2, 3, 4, 5, 10, *society, 7, 8, 9] {
                    writer.u32(value);
                }
            }
            Self::Friends(value) => {
                value.write(&mut writer, limits.max_entries, limits.max_string_bytes)?
            }
            Self::Squelch(value) => value.write(
                &mut writer,
                limits.max_entries,
                limits.max_filters,
                limits.max_string_bytes,
            )?,
            Self::ChannelList(names) => write_names(&mut writer, names, limits)?,
            Self::ChannelIndex(lists) => {
                if lists.len() > 3 {
                    return Err(WireError::LimitExceeded);
                }
                for names in *lists {
                    write_names(&mut writer, names, limits)?;
                }
            }
        }
        if writer.position() > limits.max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(writer.into_bytes())
    }
}
fn write_names(
    writer: &mut Writer,
    names: &[String],
    limits: SocialCodecLimits,
) -> Result<(), WireError> {
    if names.len() > limits.max_entries || names.len() > u32::MAX as usize {
        return Err(WireError::LimitExceeded);
    }
    writer.u32(names.len() as u32);
    for name in names {
        string(writer, name, limits.max_string_bytes)?;
    }
    Ok(())
}
