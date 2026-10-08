//! Projection of accepted recipient lists; never re-evaluates permissions or delivers privately to the chat API.
use crate::{
    BatchLimits, EventSequencer, SequenceKind, Sequences, SessionBatch, SessionProjectionError,
};
use bace_gameplay_api::{
    CharacterBinding,
    social::{ChatDelivery, SocialEvent},
};
use bace_wire::{ChatMessage, SocialCodecLimits};
impl EventSequencer {
    pub fn project_social_event(
        &mut self,
        binding: CharacterBinding,
        event: &SocialEvent,
        sequences: &mut Sequences,
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        if matches!(
            event,
            SocialEvent::Fellowship { .. }
                | SocialEvent::FellowshipLeft { .. }
                | SocialEvent::Allegiance { .. }
                | SocialEvent::Confirmation { .. }
        ) {
            return self.project_group_event(binding, event, limits);
        }
        if let SocialEvent::EquipmentMana {
            recipient,
            name,
            depleted,
        } = event
        {
            recipient_check(binding, *recipient)?;
            let text = if *depleted {
                format!("Your {name} is out of Mana.")
            } else {
                format!("Your {name} is low on Mana.")
            };
            if text.len() > limits.max_string_bytes {
                return Err(SessionProjectionError::Limit);
            }
            let mut messages = Vec::new();
            let mut total = 0;
            crate::session_output::push(
                &mut messages,
                &mut total,
                9,
                ChatMessage::System {
                    text: &text,
                    chat_type: 7,
                }
                .encode()?,
                limits,
            )?;
            if *depleted {
                let sound = bace_wire::CombatEffect::Sound {
                    object_id: recipient.0,
                    sound_id: 0x97,
                    volume: 1.,
                }
                .encode(limits.max_string_bytes, limits.max_message_bytes)?;
                crate::session_output::push(&mut messages, &mut total, 10, sound, limits)?;
            }
            return Ok(SessionBatch { binding, messages });
        }
        let l = SocialCodecLimits {
            max_message_bytes: limits.max_message_bytes,
            max_entries: 4096,
            max_filters: 4,
            max_string_bytes: limits.max_string_bytes,
        };
        let mut event_count = 0;
        let mut property = None;
        let mut queue = 9;
        let bytes = match event {
            SocialEvent::Chat {
                accepted,
                recipients,
                wire,
            } => {
                if !recipients.contains(&binding.actor) {
                    return Err(SessionProjectionError::WrongBinding);
                }
                if accepted.sender_name.len() > limits.max_string_bytes
                    || accepted.text.len() > limits.max_string_bytes
                {
                    return Err(SessionProjectionError::Limit);
                }
                match wire {
                    ChatDelivery::Speech => ChatMessage::Speech {
                        text: &accepted.text,
                        sender_name: &accepted.sender_name,
                        sender_id: accepted.sender.0,
                        chat_type: 2,
                    }
                    .encode()?,
                    ChatDelivery::Emote => ChatMessage::Emote {
                        text: &accepted.text,
                        sender_name: &accepted.sender_name,
                        sender_id: accepted.sender.0,
                    }
                    .encode()?,
                    ChatDelivery::SoulEmote => ChatMessage::SoulEmote {
                        text: &accepted.text,
                        sender_name: &accepted.sender_name,
                        sender_id: accepted.sender.0,
                    }
                    .encode()?,
                    ChatDelivery::Tell => {
                        event_count = 1;
                        bace_wire::SocialEvent::Tell {
                            text: &accepted.text,
                            sender_name: &accepted.sender_name,
                            sender_id: accepted.sender.0,
                            target_id: binding.actor.0,
                            chat_type: 3,
                        }
                        .encode(binding.actor.0, self.next, l)?
                    }
                    ChatDelivery::LegacyChannel(channel) => {
                        event_count = 1;
                        bace_wire::SocialEvent::ChannelBroadcast {
                            channel: *channel,
                            sender_name: if accepted.sender == binding.actor {
                                ""
                            } else {
                                &accepted.sender_name
                            },
                            text: &accepted.text,
                        }
                        .encode(binding.actor.0, self.next, l)?
                    }
                    ChatDelivery::Turbine { channel, chat_type } => {
                        queue = 4;
                        bace_wire::TurbineChatEvent {
                            channel: *channel,
                            sender_name: accepted.sender_name.clone(),
                            text: accepted.text.clone(),
                            sender_id: accepted.sender.0,
                            chat_type: *chat_type,
                        }
                        .encode(l.max_message_bytes, 255)?
                    }
                }
            }
            SocialEvent::TurbineEcho {
                recipient,
                sender_name,
                channel,
                chat_type,
                text,
            } => {
                recipient_check(binding, *recipient)?;
                queue = 4;
                bace_wire::TurbineChatEvent {
                    channel: *channel,
                    chat_type: *chat_type,
                    sender_id: recipient.0,
                    sender_name: sender_name.clone(),
                    text: text.clone(),
                }
                .encode(l.max_message_bytes, 255)?
            }
            SocialEvent::Transient { recipient, text } => {
                recipient_check(binding, *recipient)?;
                event_count = 1;
                bace_wire::SocialEvent::Transient(text).encode(binding.actor.0, self.next, l)?
            }
            SocialEvent::System {
                recipient,
                text,
                chat_type,
            } => {
                recipient_check(binding, *recipient)?;
                if text.len() > limits.max_string_bytes {
                    return Err(SessionProjectionError::Limit);
                }
                ChatMessage::System {
                    text,
                    chat_type: *chat_type,
                }
                .encode()?
            }
            SocialEvent::Error {
                recipient,
                code,
                argument,
            } => {
                recipient_check(binding, *recipient)?;
                event_count = 1;
                match argument {
                    None => bace_wire::SimpleGameEvent::WeenieError(*code)
                        .encode(binding.actor.0, self.next),
                    Some(text) => bace_wire::GroupEvent::ErrorWithString { code: *code, text }
                        .encode(binding.actor.0, self.next, l)?,
                }
            }
            SocialEvent::Friends {
                recipient,
                kind,
                entries,
            } => {
                recipient_check(binding, *recipient)?;
                event_count = 1;
                if entries.len() > l.max_entries {
                    return Err(SessionProjectionError::Limit);
                }
                let kind = match kind {
                    0 => bace_wire::FriendsUpdateKind::Full,
                    1 => bace_wire::FriendsUpdateKind::Added,
                    2 => bace_wire::FriendsUpdateKind::Removed,
                    4 => bace_wire::FriendsUpdateKind::StatusChanged,
                    _ => return Err(SessionProjectionError::InvalidProjection),
                };
                let table = bace_wire::FriendsUpdate {
                    kind,
                    friends: entries
                        .iter()
                        .map(|e| bace_wire::FriendEntry {
                            object_id: e.character.0,
                            online: e.online,
                            name: e.name.clone(),
                        })
                        .collect(),
                };
                bace_wire::SocialEvent::Friends(&table).encode(binding.actor.0, self.next, l)?
            }
            SocialEvent::Squelches {
                recipient,
                entries,
                global_mask,
            } => {
                recipient_check(binding, *recipient)?;
                event_count = 1;
                if entries.len() > l.max_entries {
                    return Err(SessionProjectionError::Limit);
                }
                let mut characters = std::collections::BTreeMap::new();
                for entry in entries.iter().filter(|e| e.account.is_none()) {
                    if characters
                        .insert(
                            entry.character.0,
                            bace_wire::SquelchInfo {
                                filters: vec![entry.mask; 4],
                                player_name: entry.name.clone(),
                                account: false,
                            },
                        )
                        .is_some()
                    {
                        return Err(SessionProjectionError::InvalidProjection);
                    }
                }
                for entry in entries.iter().filter(|e| e.account.is_some()) {
                    characters
                        .entry(entry.character.0)
                        .and_modify(|v| v.account = true)
                        .or_insert_with(|| bace_wire::SquelchInfo {
                            filters: vec![u32::MAX; 4],
                            player_name: entry.name.clone(),
                            account: true,
                        });
                }
                let table = bace_wire::SquelchDatabase {
                    characters: characters
                        .into_iter()
                        .map(|(object_id, info)| bace_wire::SquelchEntry { object_id, info })
                        .collect(),
                    global: bace_wire::SquelchInfo {
                        filters: if *global_mask == 0 {
                            Vec::new()
                        } else {
                            vec![*global_mask]
                        },
                        player_name: String::new(),
                        account: false,
                    },
                };
                bace_wire::SocialEvent::Squelch(&table).encode(binding.actor.0, self.next, l)?
            }
            SocialEvent::Channels {
                recipient,
                allegiance,
                society,
            } => {
                recipient_check(binding, *recipient)?;
                event_count = 1;
                bace_wire::SocialEvent::TurbineChannels {
                    allegiance: *allegiance,
                    society: *society,
                }
                .encode(binding.actor.0, self.next, l)?
            }
            SocialEvent::TurbineResponse {
                recipient,
                context_id,
                ..
            } => {
                recipient_check(binding, *recipient)?;
                queue = 4;
                bace_wire::TurbineChatResponse {
                    context_id: *context_id,
                }
                .encode()
            }
            SocialEvent::Afk { recipient, enabled } => {
                recipient_check(binding, *recipient)?;
                property = Some((SequenceKind::PropertyBool, 110));
                bace_wire::PropertyUpdate {
                    sequence: (sequences.current(SequenceKind::PropertyBool, 110) as u8)
                        .wrapping_add(1),
                    object_id: None,
                    property: 110,
                    value: bace_wire::PropertyValue::Bool(*enabled),
                }
                .encode()?
            }
            _ => return Err(SessionProjectionError::InvalidProjection),
        };
        let mut messages = Vec::new();
        let mut total = 0;
        crate::session_output::push(&mut messages, &mut total, queue, bytes, limits)?;
        if let Some((kind, id)) = property {
            sequences
                .advance(kind, id)
                .map_err(|_| SessionProjectionError::Limit)?;
        }
        self.next = self.next.wrapping_add(event_count);
        Ok(SessionBatch { binding, messages })
    }
}
fn recipient_check(
    binding: CharacterBinding,
    recipient: bace_types::EntityId,
) -> Result<(), SessionProjectionError> {
    if binding.actor != recipient {
        Err(SessionProjectionError::WrongBinding)
    } else {
        Ok(())
    }
}
