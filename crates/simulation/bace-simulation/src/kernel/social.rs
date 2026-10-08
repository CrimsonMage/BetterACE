//! Authenticated bounded social routing, with accepted output retained until delivery.
use super::Kernel;
use bace_gameplay_api::{
    ActionContext,
    social::{
        ChatChannel, ChatDelivery as W, SocialError as E, SocialEvent, SocialFriend, SocialOutcome,
        SocialRequest,
    },
};
use bace_social::{SocialPreferences, SocialPresence};
use bace_types::EntityId;
impl Kernel {
    pub fn set_social_epoch(&mut self, unix_seconds: i64) -> Result<(), E> {
        if self.social.has_state() || unix_seconds < 0 {
            return Err(E::Invalid);
        }
        self.social.epoch = unix_seconds;
        Ok(())
    }
    pub fn register_social_presence(
        &mut self,
        presence: SocialPresence,
        preferences: SocialPreferences,
    ) -> Result<(), E> {
        if presence.online && self.characters.get(presence.identity.character).is_none() {
            return Err(E::Missing);
        }
        let actor = presence.identity.character;
        let online = presence.online;
        if online
            && preferences
                .friends
                .iter()
                .any(|id| self.social.directory.presence(*id).is_none())
        {
            return Err(E::Missing);
        }
        let recipients = self.social.directory.watchers(actor).collect::<Vec<_>>();
        if self.social.events.len() + recipients.len() + usize::from(online) > self.social.capacity
        {
            return Err(E::Capacity);
        }
        self.social.directory.register(presence, preferences)?;
        if online {
            self.emit_friends(actor, 0)?;
        }
        for recipient in recipients {
            self.emit_friend_entry(recipient, actor, 4)?;
        }
        Ok(())
    }
    pub fn refresh_social_presence(&mut self, presence: SocialPresence) -> Result<(), E> {
        let actor = presence.identity.character;
        let old = self.social.directory.presence(actor).ok_or(E::Missing)?;
        let changed =
            (old.online && !old.appear_offline) != (presence.online && !presence.appear_offline);
        let recipients = if changed {
            self.social.directory.watchers(actor).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if self.social.events.len() + recipients.len() > self.social.capacity {
            return Err(E::Capacity);
        }
        self.social.directory.update_presence(presence)?;
        for recipient in recipients {
            self.emit_friend_entry(recipient, actor, 4)?;
        }
        Ok(())
    }
    pub fn social_preferences(&self, actor: EntityId) -> Option<&SocialPreferences> {
        self.social.directory.preferences(actor)
    }
    pub fn social_presence(&self, actor: EntityId) -> Option<&SocialPresence> {
        self.social.directory.presence(actor)
    }
    pub(in crate::kernel) fn can_take_social_presence(&self, actor: EntityId) -> Result<(), E> {
        if self.social.directory.preferences(actor).is_none() {
            return Err(E::Missing);
        }
        if self.allegiances.cached_skills.contains_key(&actor) {
            return Err(E::Busy);
        }
        if self
            .allegiances
            .pending
            .as_ref()
            .is_some_and(|p| p.actor == actor || p.credits.iter().any(|c| c.actor == actor))
        {
            return Err(E::Busy);
        }
        if self.social.events.len() + self.social.directory.watchers(actor).count() + 2
            > self.social.capacity
        {
            return Err(E::Capacity);
        }
        if let Some(group) = self.fellowships.membership(actor) {
            if self.social.events.len().saturating_add(2) > self.social.capacity {
                return Err(E::Capacity);
            }
            if group.leader() == actor && group.members().len() > 1 && self.social.random.is_none()
            {
                return Err(E::Missing);
            }
            if group.revision() == u64::MAX {
                return Err(E::Overflow);
            }
            for member in group.members() {
                self.validate_fellow_projection(*member)?;
            }
            if self.social_now()? < 0 {
                return Err(E::Invalid);
            }
        }
        Ok(())
    }
    pub fn take_social_presence(&mut self, actor: EntityId) -> Result<SocialPreferences, E> {
        self.can_take_social_presence(actor)?;
        let recipients = self.social.directory.watchers(actor).collect::<Vec<_>>();
        if self.fellowships.membership(actor).is_some() {
            let now = u64::try_from(self.social_now()?).map_err(|_| E::Invalid)?;
            self.remove_fellowship_member(actor, false, now)?;
        }
        let prefs = self.social.directory.unregister(actor)?;
        // A pending invitation can target the departing actor or originate from
        // them. Neither may be accepted by a later session after this handoff.
        self.social.confirmations.retain(|recipient, pending| {
            if *recipient == actor {
                return false;
            }
            match &pending.confirmation {
                crate::social::SocialConfirmation::Fellowship {
                    inviter, target, ..
                } => *inviter != actor && *target != actor,
                crate::social::SocialConfirmation::Allegiance { vassal, patron, .. } => {
                    *vassal != actor && *patron != actor
                }
            }
        });
        self.social.chat_eligibility.remove(&actor);
        self.fellowships.panels.remove(&actor);
        self.allegiances.listeners.remove(&actor);
        for recipient in recipients {
            self.emit_friend_entry(recipient, actor, 4)?;
        }
        Ok(prefs)
    }
    pub fn peek_social_event(&self) -> Option<&SocialEvent> {
        self.social.events.front()
    }
    pub fn take_social_event(&mut self) -> Option<SocialEvent> {
        self.social.events.pop_front()
    }
    pub fn peek_social_outcome(&self) -> Option<&SocialOutcome> {
        self.social.outcomes.front()
    }
    pub fn take_social_outcome(&mut self) -> Option<SocialOutcome> {
        self.social.outcomes.pop_front()
    }
    pub fn has_social_state(&self) -> bool {
        self.social.has_state() || self.fellowships.has_state() || self.allegiances.has_state()
    }
    pub(in crate::kernel) fn social_now(&self) -> Result<i64, E> {
        self.social
            .epoch
            .checked_add(i64::try_from(self.tick / 30).map_err(|_| E::Overflow)?)
            .ok_or(E::Overflow)
    }
    pub(in crate::kernel) fn authorize_social(&mut self, context: ActionContext) -> Result<(), E> {
        if self.characters.reserved(context.actor)
            || self.inventory.reserved(context.actor)
            || self.npcs.reserved(context.actor)
            || self.housing.reserved(context.actor)
        {
            return Err(E::Busy);
        }
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|_| E::Stale)?;
        let source = self
            .social
            .directory
            .presence(context.actor)
            .filter(|p| p.online)
            .ok_or(E::Offline)?;
        if source.identity.account != context.account {
            return Err(E::Forbidden);
        }
        Ok(())
    }
    pub fn request_social(
        &mut self,
        context: ActionContext,
        request: SocialRequest,
    ) -> Result<(), E> {
        if self.social.outcomes.len() >= self.social.capacity
            || self.social.events.len().saturating_add(4) > self.social.capacity
        {
            return Err(E::Capacity);
        }
        let authorization = self.authorize_social(context);
        let retryable = authorization == Err(E::Busy);
        let result = authorization.and_then(|()| self.apply_social(context.actor, request));
        self.social.outcomes.push_back(SocialOutcome {
            context,
            result,
            retryable,
        });
        result
    }
    pub(super) fn apply_social(
        &mut self,
        actor: EntityId,
        request: SocialRequest,
    ) -> Result<(), E> {
        if matches!(
            &request,
            SocialRequest::Talk(_)
                | SocialRequest::Emote(_)
                | SocialRequest::SoulEmote(_)
                | SocialRequest::Tell { .. }
                | SocialRequest::Turbine { .. }
        ) && self
            .social
            .directory
            .presence(actor)
            .is_some_and(|p| p.gagged)
        {
            self.emit_gag_error(actor);
            return Err(E::Gagged);
        }
        match request {
            SocialRequest::Talk(text) => {
                self.route_social(actor, ChatChannel::Local, text, None, (W::Speech, None))
            }
            SocialRequest::Emote(text) => {
                self.route_social(actor, ChatChannel::Emote, text, None, (W::Emote, None))
            }
            SocialRequest::SoulEmote(text) => {
                let p = self.social.directory.presence(actor).ok_or(E::Missing)?;
                if p.olthoi && !p.no_olthoi_talk {
                    return Ok(());
                }
                self.route_social(
                    actor,
                    ChatChannel::SoulEmote,
                    text,
                    None,
                    (W::SoulEmote, None),
                )
            }
            SocialRequest::Tell { text, target_name } => {
                let target = self
                    .social
                    .directory
                    .by_name(target_name.trim())
                    .filter(|p| p.online)
                    .map(|p| p.identity.character);
                let Some(target) = target else {
                    self.emit_chat_error(actor, 0x52b, None);
                    return Err(E::Offline);
                };
                self.route_social(
                    actor,
                    ChatChannel::Tell,
                    text,
                    Some((target, false)),
                    (W::Tell, None),
                )
            }
            SocialRequest::TalkDirect { text, target } => self.route_social(
                actor,
                ChatChannel::Tell,
                text,
                Some((target, true)),
                (W::Tell, None),
            ),
            SocialRequest::Channel { channel, text } => {
                let mapped = legacy_channel(channel)?;
                if channel == 0x2000000 && self.allegiances.registry.permission(actor) < 1 {
                    let code = if self.allegiance_relation(actor).is_none() {
                        0x414
                    } else {
                        0x535
                    };
                    self.emit_chat_error(actor, code, None);
                    return Err(E::Forbidden);
                }
                if channel < 2048
                    && bace_social::legal_legacy_channel(
                        self.social
                            .directory
                            .presence(actor)
                            .ok_or(E::Missing)?
                            .access,
                        channel,
                    )
                    && !self
                        .social
                        .directory
                        .preferences(actor)
                        .is_some_and(|p| p.channels.contains(&channel))
                {
                    return Ok(());
                }
                let result =
                    self.route_social(actor, mapped, text, None, (W::LegacyChannel(channel), None));
                if let Err(error) = result {
                    let code = match error {
                        E::NotMember if channel == 2048 => Some(0x50f),
                        E::NotMember => Some(0x414),
                        E::Forbidden => Some(0x423),
                        _ => None,
                    };
                    if let Some(code) = code {
                        self.emit_chat_error(actor, code, None);
                    }
                }
                result
            }
            SocialRequest::Turbine {
                context_id,
                dispatch,
                channel,
                chat_type,
                text,
            } => {
                let original_channel = channel;
                let original_chat_type = chat_type;
                let (channel, chat_type) =
                    bace_social::adjust_turbine_channel(dispatch, channel, chat_type)?;
                let mapped = match channel {
                    2 => ChatChannel::General,
                    3 => ChatChannel::Trade,
                    4 => ChatChannel::Lfg,
                    5 => ChatChannel::Roleplay,
                    6..=9 => {
                        let society = self
                            .social
                            .directory
                            .presence(actor)
                            .ok_or(E::Missing)?
                            .society;
                        if !(7..=9).contains(&society) {
                            self.social.events.push_back(SocialEvent::System {
                                recipient: actor,
                                text: "You do not belong to a society.".into(),
                                chat_type: 0,
                            });
                            return Err(E::Forbidden);
                        }
                        ChatChannel::Society(society)
                    }
                    10 => ChatChannel::Olthoi,
                    _ => {
                        if self
                            .allegiance_relation(actor)
                            .and_then(|r| self.allegiances.registry.metadata(r.monarch))
                            .is_some_and(|_| channel == 1 || channel > 10)
                        {
                            ChatChannel::Allegiance
                        } else {
                            return Err(E::Forbidden);
                        }
                    }
                };
                if matches!(
                    mapped,
                    ChatChannel::General
                        | ChatChannel::Trade
                        | ChatChannel::Lfg
                        | ChatChannel::Roleplay
                        | ChatChannel::Olthoi
                ) {
                    let source = self.social.directory.presence(actor).ok_or(E::Missing)?;
                    if (channel == 10) != source.olthoi {
                        return Err(E::Forbidden);
                    }
                    if text.encode_utf16().count() > 255 || text.contains('\0') {
                        return Err(E::Invalid);
                    }
                    let (mut eligibility, at) = self
                        .social
                        .chat_eligibility
                        .get(&actor)
                        .copied()
                        .unwrap_or((Default::default(), self.tick));
                    eligibility.player_age_seconds = eligibility
                        .player_age_seconds
                        .saturating_add(self.tick.saturating_sub(at) / 30);
                    let level = self
                        .characters
                        .native_services(actor)
                        .map_or(0, |s| s.level);
                    let any_listener = self.social.directory.online().any(|id| {
                        self.social
                            .directory
                            .presence(id)
                            .is_some_and(|p| match original_channel {
                                2 => p.listen_general,
                                3 => p.listen_trade,
                                4 => p.listen_lfg,
                                5 => p.listen_roleplay,
                                _ => true,
                            })
                    });
                    let gate = self.social.chat_policy.evaluate(
                        if any_listener { original_channel } else { 0 },
                        channel,
                        eligibility,
                        level,
                        self.social_now()?,
                    );
                    if gate != bace_social::PublicChatGate::Deliver {
                        if gate == bace_social::PublicChatGate::Echo
                            || self.social.chat_policy.echo_reject
                        {
                            self.social.events.push_back(SocialEvent::TurbineEcho {
                                recipient: actor,
                                sender_name: source.identity.name.clone(),
                                channel,
                                chat_type,
                                text,
                            });
                        }
                        if let bace_social::PublicChatGate::Reject {
                            reason,
                            inform: true,
                        } = &gate
                        {
                            let kind = match original_chat_type {
                                1 => "Allegiance",
                                2 => "General",
                                3 => "Trade",
                                4 => "LFG",
                                5 => "Roleplay",
                                6 => "Society",
                                7 => "SocietyCelHan",
                                8 => "SocietyEldWeb",
                                9 => "SocietyRadBlo",
                                10 => "Olthoi",
                                _ => "Undef",
                            };
                            let suffix = if reason.is_empty() {
                                String::new()
                            } else {
                                format!(" for you {reason}")
                            };
                            let text = format!("{kind} is currently disabled{suffix}.");
                            self.social.events.push_back(SocialEvent::Transient {
                                recipient: actor,
                                text: text.clone(),
                            });
                            self.social.events.push_back(SocialEvent::System {
                                recipient: actor,
                                text,
                                chat_type: 0,
                            });
                        }
                        self.social.events.push_back(SocialEvent::TurbineResponse {
                            recipient: actor,
                            context_id,
                            dispatch: 1,
                            result: 0,
                            chat_type,
                        });
                        return Ok(());
                    }
                }
                self.route_social(
                    actor,
                    mapped,
                    text,
                    None,
                    (W::Turbine { channel, chat_type }, Some(original_channel)),
                )?;
                self.social.events.push_back(SocialEvent::TurbineResponse {
                    recipient: actor,
                    context_id,
                    dispatch,
                    result: 0,
                    chat_type,
                });
                Ok(())
            }
            SocialRequest::SetAfk(enabled) => {
                let mut presence = self
                    .social
                    .directory
                    .presence(actor)
                    .ok_or(E::Missing)?
                    .clone();
                presence.afk = enabled;
                self.social.directory.update_presence(presence)?;
                self.social.events.push_back(SocialEvent::Afk {
                    recipient: actor,
                    enabled,
                });
                Ok(())
            }
            request => {
                let before = self
                    .social
                    .directory
                    .preferences(actor)
                    .ok_or(E::Missing)?
                    .clone();
                let proposed = self.social.directory.propose_preferences(actor, &request);
                let feedback = self.social.directory.squelch_feedback(
                    actor,
                    &request,
                    proposed.as_ref().map(|_| ()).map_err(|e| *e),
                );
                if let Err(error) = proposed {
                    if matches!(request, SocialRequest::AddFriend(_)) {
                        let text = match error {
                            E::Invalid => "Sorry, but you can't be friends with yourself.",
                            E::Duplicate => "That character is already in your friends list",
                            E::Missing => "That character does not exist",
                            _ => "Unable to update friends list.",
                        };
                        self.social.events.push_back(SocialEvent::System {
                            recipient: actor,
                            text: text.into(),
                            chat_type: 0,
                        });
                    }
                    if let Some(text) = feedback {
                        self.social.events.push_back(SocialEvent::System {
                            recipient: actor,
                            text,
                            chat_type: 0,
                        });
                    }
                    if matches!(request, SocialRequest::CharacterSquelch { .. })
                        && matches!(error, E::Duplicate)
                    {
                        let prefs = self.social.directory.preferences(actor).ok_or(E::Missing)?;
                        self.social.events.push_back(SocialEvent::Squelches {
                            recipient: actor,
                            entries: prefs.squelches.clone(),
                            global_mask: prefs.global_mask,
                        });
                    }
                    return Err(error);
                }
                let after = proposed?;
                let changed = before != after;
                if changed {
                    if !self
                        .characters
                        .touch_auxiliary(actor)
                        .map_err(|_| E::Overflow)?
                    {
                        return Err(E::Busy);
                    }
                    self.social
                        .directory
                        .adopt_preferences(actor, &before, after)?;
                }
                if let Some(text) = feedback {
                    self.social.events.push_back(SocialEvent::System {
                        recipient: actor,
                        text,
                        chat_type: 0,
                    });
                }
                match request {
                    SocialRequest::AddFriend(name) => {
                        let target = self
                            .social
                            .directory
                            .by_name(&name)
                            .ok_or(E::Missing)?
                            .identity
                            .character;
                        self.emit_friend_entry(actor, target, 1)?;
                        let name = &self
                            .social
                            .directory
                            .presence(target)
                            .ok_or(E::Missing)?
                            .identity
                            .name;
                        self.social.events.push_back(SocialEvent::System {
                            recipient: actor,
                            text: format!("{name} has been added to your friends list."),
                            chat_type: 0,
                        });
                    }
                    SocialRequest::RemoveFriend(target) => {
                        self.emit_friend_entry(actor, target, 2)?
                    }
                    SocialRequest::RemoveAllFriends => self.emit_friends(actor, 0)?,
                    SocialRequest::CharacterSquelch { .. }
                    | SocialRequest::AccountSquelch { .. }
                    | SocialRequest::GlobalSquelch { .. }
                        if changed || !matches!(request, SocialRequest::GlobalSquelch { .. }) =>
                    {
                        let prefs = self.social.directory.preferences(actor).ok_or(E::Missing)?;
                        self.social.events.push_back(SocialEvent::Squelches {
                            recipient: actor,
                            entries: prefs.squelches.clone(),
                            global_mask: prefs.global_mask,
                        });
                    }
                    _ => {}
                }
                Ok(())
            }
        }
    }
    fn emit_friend_entry(
        &mut self,
        recipient: EntityId,
        actor: EntityId,
        kind: u32,
    ) -> Result<(), E> {
        if self.social.events.len() >= self.social.capacity {
            return Err(E::Capacity);
        }
        let p = self.social.directory.presence(actor).ok_or(E::Missing)?;
        self.social.events.push_back(SocialEvent::Friends {
            recipient,
            kind,
            entries: vec![SocialFriend {
                character: actor,
                name: p.identity.name.clone(),
                online: p.online && !p.appear_offline,
            }],
        });
        Ok(())
    }
    pub(in crate::kernel) fn emit_friends(&mut self, actor: EntityId, kind: u32) -> Result<(), E> {
        if self.social.events.len() >= self.social.capacity {
            return Err(E::Capacity);
        }
        let prefs = self.social.directory.preferences(actor).ok_or(E::Missing)?;
        let entries = prefs
            .friends
            .iter()
            .map(|id| self.social.directory.presence(*id).ok_or(E::Missing))
            .map(|p| {
                let p = p?;
                Ok(SocialFriend {
                    character: p.identity.character,
                    name: p.identity.name.clone(),
                    online: p.online && !p.appear_offline,
                })
            })
            .collect::<Result<Vec<_>, E>>()?;
        self.social.events.push_back(SocialEvent::Friends {
            recipient: actor,
            kind,
            entries,
        });
        Ok(())
    }
    pub(in crate::kernel) fn route_social(
        &mut self,
        actor: EntityId,
        channel: ChatChannel,
        text: String,
        target: Option<(EntityId, bool)>,
        wire: (W, Option<u32>),
    ) -> Result<(), E> {
        let (wire, public_listener) = wire;
        let direct = target.is_some_and(|(_, direct)| direct);
        let target = target.map(|(target, _)| target);
        if direct
            && target.is_none_or(|target| {
                self.world
                    .actor_state(actor)
                    .ok()
                    .zip(self.world.actor_state(target).ok())
                    .is_none_or(|((a, _), (b, _))| a.0 >> 16 != b.0 >> 16)
            })
        {
            self.emit_chat_error(actor, 0x52b, None);
            return Err(E::Offline);
        }
        let now = self.social_now()?;
        let candidates = match self.social_candidates(actor, channel, target, now) {
            Ok(candidates) => candidates,
            Err(error) => {
                if channel == ChatChannel::Tell {
                    self.emit_chat_error(actor, 0x52b, None);
                }
                return Err(error);
            }
        };
        let first_sequence = self.social.sequence.checked_add(1).ok_or(E::Overflow)?;
        let patron_delivery = if channel == ChatChannel::Legacy(0x1000000) {
            let patron = self
                .allegiances
                .registry
                .node(actor)
                .and_then(|n| n.patron)
                .ok_or(E::NotMember)?;
            let delivery = self.social.directory.route(
                actor,
                ChatChannel::Legacy(8192),
                text.clone(),
                &[patron],
                first_sequence,
                now,
            )?;
            (!delivery.recipients.is_empty()).then_some(delivery)
        } else {
            None
        };
        let sequence = first_sequence
            .checked_add(u64::from(patron_delivery.is_some()))
            .ok_or(E::Overflow)?;
        if channel == ChatChannel::Tell {
            let gagged = self
                .social
                .directory
                .presence(actor)
                .ok_or(E::Missing)?
                .gagged;
            if text.encode_utf16().count() > 255 || text.contains('\0') {
                return Err(E::Invalid);
            }
            if let Some(target) = target.filter(|id| direct || *id != actor) {
                let name = &self
                    .social
                    .directory
                    .presence(target)
                    .ok_or(E::Missing)?
                    .identity
                    .name;
                self.social.events.push_back(SocialEvent::System {
                    recipient: actor,
                    text: format!("You tell {name}, \"{text}\""),
                    chat_type: 4,
                });
            }
            if gagged {
                self.emit_gag_error(actor);
                return Err(E::Gagged);
            }
        }
        let result = if let Some(original) = public_listener {
            self.social.directory.route_with_public_listener(
                actor,
                channel,
                text,
                &candidates,
                (sequence, now),
                original,
            )
        } else {
            self.social
                .directory
                .route(actor, channel, text, &candidates, sequence, now)
        };
        let delivery = match result {
            Ok(delivery) => delivery,
            Err(error) => {
                if error == E::Squelched
                    && let Some(target) = target
                {
                    let name = &self
                        .social
                        .directory
                        .presence(target)
                        .ok_or(E::Missing)?
                        .identity
                        .name;
                    self.emit_chat_error(actor, 0x51f, Some(format!("{name} has you squelched.")));
                }
                return Err(error);
            }
        };
        self.social.sequence = sequence;
        if !direct
            && let Some(target) = target
            && self
                .social
                .directory
                .presence(target)
                .is_some_and(|p| p.afk)
        {
            let p = self
                .social
                .directory
                .preferences(target)
                .ok_or(E::Missing)?;
            let name = &self
                .social
                .directory
                .presence(target)
                .ok_or(E::Missing)?
                .identity
                .name;
            self.social.events.push_back(SocialEvent::Error {
                recipient: actor,
                code: 0x55e,
                argument: Some(format!(
                    "{name} is away: {}",
                    if p.afk_message.trim().is_empty() {
                        "I am currently away from the keyboard."
                    } else {
                        &p.afk_message
                    }
                )),
            });
        }
        if let Some(patron) = patron_delivery {
            self.social.events.push_back(SocialEvent::Chat {
                accepted: patron.accepted,
                wire: W::LegacyChannel(8192),
                recipients: patron.recipients,
            });
        }
        self.social.events.push_back(SocialEvent::Chat {
            accepted: delivery.accepted,
            wire,
            recipients: delivery.recipients,
        });
        Ok(())
    }
    pub(super) fn emit_chat_error(
        &mut self,
        recipient: EntityId,
        code: u32,
        argument: Option<String>,
    ) {
        self.social.events.push_back(SocialEvent::Error {
            recipient,
            code,
            argument,
        });
    }
    fn emit_gag_error(&mut self, recipient: EntityId) {
        let text =
            "You are unable to talk locally, globally, or send tells because you have been gagged.";
        self.social.events.push_back(SocialEvent::Transient {
            recipient,
            text: text.into(),
        });
        self.social.events.push_back(SocialEvent::System {
            recipient,
            text: text.into(),
            chat_type: 20,
        });
    }
    fn social_candidates(
        &self,
        actor: EntityId,
        channel: ChatChannel,
        target: Option<EntityId>,
        now: i64,
    ) -> Result<Vec<EntityId>, E> {
        match channel {
            ChatChannel::Tell => {
                let target = target.ok_or(E::Invalid)?;
                self.social
                    .directory
                    .presence(target)
                    .filter(|p| p.online)
                    .ok_or(E::Offline)?;
                Ok(vec![target])
            }
            ChatChannel::Fellowship => {
                Ok(self.fellowships.roster(actor).ok_or(E::NotMember)?.to_vec())
            }
            ChatChannel::Allegiance => {
                let relation = self.allegiance_relation(actor).ok_or(E::NotMember)?;
                let metadata = self
                    .allegiances
                    .registry
                    .metadata(relation.monarch)
                    .ok_or(E::Missing)?;
                if !metadata.chat_allowed(actor, now) {
                    return Err(E::Gagged);
                }
                Ok(self
                    .social
                    .directory
                    .online()
                    .filter(|id| {
                        self.allegiances
                            .registry
                            .node(*id)
                            .is_some_and(|n| n.monarch == relation.monarch)
                            && metadata.chat_allowed(*id, now)
                    })
                    .collect())
            }
            ChatChannel::Legacy(0x2000000) => {
                let relation = self.allegiance_relation(actor).ok_or(E::NotMember)?;
                Ok(self
                    .social
                    .directory
                    .online()
                    .filter(|id| {
                        self.allegiances
                            .registry
                            .node(*id)
                            .is_some_and(|n| n.monarch == relation.monarch)
                    })
                    .collect())
            }
            ChatChannel::Legacy(4096 | 8192 | 16384 | 0x1000000) => {
                let n = self.allegiances.registry.node(actor).ok_or(E::NotMember)?;
                if matches!(channel, ChatChannel::Legacy(4096)) && n.vassals.is_empty()
                    || matches!(channel, ChatChannel::Legacy(8192 | 0x1000000))
                        && n.patron.is_none()
                    || matches!(channel, ChatChannel::Legacy(16384)) && n.monarch == actor
                {
                    return Err(E::Forbidden);
                }
                let mut ids = match channel {
                    ChatChannel::Legacy(4096) => n.vassals.clone(),
                    ChatChannel::Legacy(8192) => n.patron.into_iter().collect(),
                    ChatChannel::Legacy(16384) => vec![n.monarch],
                    _ => n
                        .patron
                        .and_then(|id| self.allegiances.registry.node(id))
                        .map_or_else(Vec::new, |n| n.vassals.clone()),
                };
                ids.push(actor);
                Ok(ids)
            }
            ChatChannel::Local | ChatChannel::Emote | ChatChannel::SoulEmote => Ok(self
                .social
                .directory
                .online()
                .filter(|id| self.social_in_range(actor, *id, 96.0))
                .collect()),
            _ => Ok(self.social.directory.online().collect()),
        }
    }
    pub(in crate::kernel) fn social_in_range(&self, a: EntityId, b: EntityId, range: f32) -> bool {
        let (Ok((ac, ap)), Ok((bc, bp))) = (self.world.actor_state(a), self.world.actor_state(b))
        else {
            return false;
        };
        let ai = ac.0 & 0xffff >= 0x100;
        let bi = bc.0 & 0xffff >= 0x100;
        if ai != bi || ai && ac.0 >> 16 != bc.0 >> 16 {
            return false;
        }
        let mut d = ap.position() - bp.position();
        if !ai {
            d.x += (((ac.0 >> 24) & 255) as f32 - ((bc.0 >> 24) & 255) as f32) * 192.0;
            d.y += (((ac.0 >> 16) & 255) as f32 - ((bc.0 >> 16) & 255) as f32) * 192.0;
        }
        d.length_squared() <= range * range
    }
}
fn legacy_channel(channel: u32) -> Result<ChatChannel, E> {
    Ok(match channel {
        2048 => ChatChannel::Fellowship,
        0x2000000 => ChatChannel::Legacy(0x2000000),
        4 => ChatChannel::Audit,
        0x8000000 => ChatChannel::Society(7),
        0x10000000 => ChatChannel::Society(8),
        0x20000000 => ChatChannel::Society(9),
        0x40000000 => ChatChannel::Olthoi,
        1 | 2 | 8 | 16 | 32 | 512 | 1024 | 4096 | 8192 | 16384 | 0x1000000 => {
            ChatChannel::Legacy(channel)
        }
        _ => return Err(E::Invalid),
    })
}

#[cfg(test)]
#[path = "social_age_tests.rs"]
mod age_tests;

#[path = "social_lookup.rs"]
mod identity_lookup;

#[path = "social_age.rs"]
mod age;
