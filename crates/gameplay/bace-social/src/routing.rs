//! Routing plans use authenticated identities and owner-supplied relationship candidates.
use crate::{SocialDirectory, legal_legacy_channel};
use bace_gameplay_api::social::{AcceptedChat, ChatChannel, SocialError as E};
use bace_types::EntityId;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatDelivery {
    pub accepted: AcceptedChat,
    pub recipients: Vec<EntityId>,
}
impl SocialDirectory {
    pub fn route(
        &self,
        sender: EntityId,
        channel: ChatChannel,
        text: String,
        candidates: &[EntityId],
        sequence: u64,
        unix_seconds: i64,
    ) -> Result<ChatDelivery, E> {
        self.route_inner(
            sender,
            channel,
            text,
            candidates,
            (sequence, unix_seconds),
            None,
        )
    }
    pub fn route_with_public_listener(
        &self,
        sender: EntityId,
        channel: ChatChannel,
        text: String,
        candidates: &[EntityId],
        time: (u64, i64),
        original_channel: u32,
    ) -> Result<ChatDelivery, E> {
        self.route_inner(
            sender,
            channel,
            text,
            candidates,
            time,
            Some(original_channel),
        )
    }
    fn route_inner(
        &self,
        sender: EntityId,
        channel: ChatChannel,
        text: String,
        candidates: &[EntityId],
        time: (u64, i64),
        public_listener: Option<u32>,
    ) -> Result<ChatDelivery, E> {
        let (sequence, unix_seconds) = time;
        let source = self
            .presence(sender)
            .filter(|p| p.online)
            .ok_or(E::Offline)?;
        if source.gagged
            && !matches!(
                channel,
                ChatChannel::Legacy(_)
                    | ChatChannel::Audit
                    | ChatChannel::Fellowship
                    | ChatChannel::Allegiance
            )
        {
            return Err(E::Gagged);
        }
        if text.encode_utf16().count() > 255 || text.contains('\0') {
            return Err(E::Invalid);
        }
        if candidates.len() > self.capacity {
            return Err(E::Capacity);
        }
        match channel {
            ChatChannel::General
            | ChatChannel::Trade
            | ChatChannel::Lfg
            | ChatChannel::Roleplay
                if source.olthoi =>
            {
                return Err(E::Forbidden);
            }
            ChatChannel::Olthoi if !source.olthoi => return Err(E::Forbidden),
            ChatChannel::Society(society)
                if source.society != society || !(7..=9).contains(&society) =>
            {
                return Err(E::Forbidden);
            }
            ChatChannel::Audit if !legal_legacy_channel(source.access, 4) => {
                return Err(E::Forbidden);
            }
            ChatChannel::Legacy(channel)
                if !matches!(channel, 2048 | 4096 | 8192 | 16384 | 0x1000000 | 0x2000000)
                    && !legal_legacy_channel(source.access, channel) =>
            {
                return Err(E::Forbidden);
            }
            _ => {}
        }
        let message_type = match channel {
            ChatChannel::Local => 2,
            ChatChannel::Tell => 3,
            ChatChannel::Emote | ChatChannel::SoulEmote => 12,
            ChatChannel::Fellowship => 19,
            ChatChannel::Allegiance
            | ChatChannel::Legacy(4096 | 8192 | 16384 | 0x1000000 | 0x2000000) => 18,
            ChatChannel::Audit | ChatChannel::Legacy(_) => 8,
            _ => 1,
        };
        let mut recipients = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for candidate in candidates {
            let Some(recipient) = self.presence(*candidate).filter(|p| p.online) else {
                continue;
            };
            if !seen.insert(*candidate) {
                continue;
            }
            // Pinned Fellow handler's else belongs to the combined sender/squelch
            // condition: each blocked member produces another sender echo.
            if channel == ChatChannel::Fellowship {
                recipients.push(
                    if *candidate == sender || self.squelched(*candidate, sender, 19) {
                        sender
                    } else {
                        *candidate
                    },
                );
                continue;
            }
            let self_echo = *candidate == sender
                && matches!(
                    channel,
                    ChatChannel::Fellowship | ChatChannel::Legacy(4096 | 8192 | 16384 | 0x1000000)
                );
            if !self_echo && self.squelched(*candidate, sender, message_type) {
                continue;
            }
            let listen = if let Some(original) = public_listener.filter(|_| {
                matches!(
                    channel,
                    ChatChannel::General
                        | ChatChannel::Trade
                        | ChatChannel::Lfg
                        | ChatChannel::Roleplay
                )
            }) {
                !recipient.olthoi
                    && match original {
                        2 => recipient.listen_general,
                        3 => recipient.listen_trade,
                        4 => recipient.listen_lfg,
                        5 => recipient.listen_roleplay,
                        _ => true,
                    }
            } else {
                match channel {
                    ChatChannel::General => recipient.listen_general && !recipient.olthoi,
                    ChatChannel::Trade => recipient.listen_trade && !recipient.olthoi,
                    ChatChannel::Lfg => recipient.listen_lfg && !recipient.olthoi,
                    ChatChannel::Roleplay => recipient.listen_roleplay && !recipient.olthoi,
                    ChatChannel::Allegiance => recipient.listen_allegiance,
                    ChatChannel::Society(society) => {
                        recipient.listen_society
                            && (recipient.society == society || recipient.access >= 5)
                    }
                    ChatChannel::Olthoi => recipient.olthoi || recipient.access >= 5,
                    ChatChannel::Audit => {
                        legal_legacy_channel(recipient.access, 4)
                            && self
                                .preferences(*candidate)
                                .is_some_and(|p| p.channels.contains(&4))
                    }
                    ChatChannel::Legacy(channel) if channel < 2048 => {
                        legal_legacy_channel(recipient.access, channel)
                            && self
                                .preferences(*candidate)
                                .is_some_and(|p| p.channels.contains(&channel))
                    }
                    _ => true,
                }
            };
            if listen {
                recipients.push(*candidate);
            }
        }
        if channel == ChatChannel::Tell && recipients.is_empty() {
            return Err(E::Squelched);
        }
        Ok(ChatDelivery {
            accepted: AcceptedChat {
                sequence,
                unix_seconds,
                sender,
                sender_name: source.identity.name.clone(),
                channel,
                text,
            },
            recipients,
        })
    }
}
