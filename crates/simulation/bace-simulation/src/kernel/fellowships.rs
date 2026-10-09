//! Fellowship membership has one owner and is shared by gameplay and client projections.
use super::Kernel;
use bace_fellowship::{Fellowship, FellowshipError};
use bace_gameplay_api::{
    ActionContext,
    social::{
        FellowSnapshot, FellowshipRequest, FellowshipSnapshot, SocialError as E, SocialEvent,
        SocialOutcome,
    },
};
use bace_types::EntityId;
impl Kernel {
    pub fn configure_social_random(
        &mut self,
        root: std::sync::Arc<bace_random::RandomRoot>,
    ) -> Result<(), E> {
        if self.fellowships.has_state() {
            return Err(E::Busy);
        }
        self.social.random = Some(root);
        Ok(())
    }
    pub fn request_fellowship(
        &mut self,
        context: ActionContext,
        request: FellowshipRequest,
    ) -> Result<(), E> {
        if self.social.outcomes.len() >= self.social.capacity
            || self.social.events.len().saturating_add(4) > self.social.capacity
        {
            return Err(E::Capacity);
        }
        let authorization = self.authorize_social(context);
        let retryable = authorization == Err(E::Busy);
        let result = authorization.and_then(|()| self.apply_fellowship(context.actor, request));
        self.social.outcomes.push_back(SocialOutcome {
            context,
            result,
            retryable,
        });
        result
    }
    fn apply_fellowship(&mut self, actor: EntityId, request: FellowshipRequest) -> Result<(), E> {
        self.validate_fellow_projection(actor)?;
        if let Some(group) = self.fellowships.membership(actor) {
            for member in group.members() {
                self.validate_fellow_projection(*member)?;
            }
        }
        let now = u64::try_from(self.social_now()?).map_err(|_| E::Invalid)?;
        match request {
            FellowshipRequest::Create { name, share_xp } => {
                let presence = self.social.directory.presence(actor).ok_or(E::Missing)?;
                if presence.olthoi {
                    return Err(E::Forbidden);
                }
                if self.fellowships.members.contains_key(&actor) {
                    return Err(E::Duplicate);
                }
                let id = self.fellowships.next.checked_add(1).ok_or(E::Overflow)?;
                let mut group =
                    Fellowship::new(id, name, actor, share_xp).map_err(map_fellow_error)?;
                group
                    .set_share_loot_at_creation(presence.share_fellowship_loot)
                    .map_err(map_fellow_error)?;
                self.fellowships.register(group)?;
                self.emit_fellowship(actor)
            }
            FellowshipRequest::Recruit(target) => {
                self.validate_fellow_projection(target)?;
                let recipient = self
                    .social
                    .directory
                    .presence(target)
                    .filter(|p| p.online)
                    .ok_or(E::Offline)?;
                if recipient.olthoi || recipient.ignore_fellowship_requests {
                    return Err(E::Forbidden);
                }
                if self.fellowships.members.contains_key(&target) {
                    return Err(E::Duplicate);
                }
                if !self.social_in_range(actor, target, 192.0) {
                    return Err(E::Invalid);
                }
                let before = self
                    .fellowships
                    .membership(actor)
                    .ok_or(E::NotMember)?
                    .clone();
                before
                    .can_recruit(actor, target, now)
                    .map_err(map_fellow_error)?;
                if recipient.auto_accept_fellowship {
                    let mut after = before.clone();
                    after
                        .recruit_confirmed(actor, target, now)
                        .map_err(map_fellow_error)?;
                    self.fellowships.replace(&before, after)?;
                    self.emit_fellowship(actor)
                } else {
                    let text = format!(
                        "{} invites you to join fellowship {}.",
                        self.social
                            .directory
                            .presence(actor)
                            .ok_or(E::Missing)?
                            .identity
                            .name,
                        before.name()
                    );
                    self.queue_social_confirmation(
                        target,
                        crate::social::SocialConfirmation::Fellowship {
                            inviter: actor,
                            target,
                            fellowship: before.id,
                            revision: before.revision(),
                        },
                        3,
                        text,
                    )?;
                    Ok(())
                }
            }
            FellowshipRequest::Confirm { token, accepted } => {
                let pending = self
                    .social
                    .confirmations
                    .get(&actor)
                    .ok_or(E::Stale)?
                    .clone();
                if pending.token != token || self.tick > pending.expires_tick {
                    return Err(E::Stale);
                }
                let crate::social::SocialConfirmation::Fellowship {
                    inviter,
                    target,
                    fellowship,
                    revision,
                } = pending.confirmation
                else {
                    return Err(E::Invalid);
                };
                self.social.confirmations.remove(&actor);
                if !accepted {
                    return Err(E::Declined);
                }
                if target != actor || self.fellowships.members.contains_key(&actor) {
                    return Err(E::Duplicate);
                }
                let before = self
                    .fellowships
                    .groups
                    .get(&fellowship)
                    .ok_or(E::Missing)?
                    .clone();
                if before.revision() != revision {
                    return Err(E::Stale);
                }
                let p = self.social.directory.presence(actor).ok_or(E::Missing)?;
                if p.olthoi
                    || p.ignore_fellowship_requests
                    || !self.social_in_range(inviter, actor, 192.0)
                {
                    return Err(E::Forbidden);
                }
                let mut after = before.clone();
                after
                    .recruit_confirmed(inviter, actor, now)
                    .map_err(map_fellow_error)?;
                self.fellowships.replace(&before, after)?;
                self.emit_fellowship(actor)
            }
            FellowshipRequest::Quit { disband } => {
                let before = self
                    .fellowships
                    .membership(actor)
                    .ok_or(E::NotMember)?
                    .clone();
                if disband {
                    if actor != before.leader() {
                        return Err(E::NotLeader);
                    }
                    // ACE Fellowship.QuitFellowship sends Disband and then this
                    // Broadcast notice to each member when loot sharing is on.
                    // Reserve the complete event batch before removing the group.
                    let needed = 1 + if before.share_loot() {
                        before.members().len()
                    } else {
                        0
                    };
                    if self.social.events.len().saturating_add(needed) > self.social.capacity {
                        return Err(E::Capacity);
                    }
                    let recipients = self.fellowships.disband(before.id)?;
                    self.social.events.push_back(SocialEvent::FellowshipLeft {
                        recipients: recipients.clone(),
                        actor,
                        dismissed: false,
                        disbanded: true,
                    });
                    if before.share_loot() {
                        for recipient in recipients {
                            self.social.events.push_back(SocialEvent::System {
                                recipient,
                                text: "You no longer have permission to loot anyone else's kills."
                                    .into(),
                                chat_type: 0,
                            });
                        }
                    }
                    Ok(())
                } else {
                    self.remove_fellowship_member(actor, false, now)
                }
            }
            FellowshipRequest::Dismiss(target) => {
                let group = self.fellowships.membership(actor).ok_or(E::NotMember)?;
                if group.leader() != actor {
                    return Err(E::NotLeader);
                }
                if target == actor {
                    return Err(E::Invalid);
                }
                if !group.members().contains(&target) {
                    return Err(E::NotMember);
                }
                self.remove_fellowship_member(target, true, now)
            }
            FellowshipRequest::AssignLeader(target) => {
                let before = self
                    .fellowships
                    .membership(actor)
                    .ok_or(E::NotMember)?
                    .clone();
                let mut after = before.clone();
                after
                    .assign_leader(actor, target)
                    .map_err(map_fellow_error)?;
                self.fellowships.replace(&before, after)?;
                self.emit_fellowship(actor)
            }
            FellowshipRequest::ChangeOpenness(open) => {
                let before = self
                    .fellowships
                    .membership(actor)
                    .ok_or(E::NotMember)?
                    .clone();
                let mut after = before.clone();
                after
                    .change_openness(actor, open)
                    .map_err(map_fellow_error)?;
                self.fellowships.replace(&before, after)?;
                self.emit_fellowship(actor)
            }
            FellowshipRequest::Panel(enabled) => {
                if enabled {
                    self.fellowships.panels.insert(actor);
                    if self.fellowships.membership(actor).is_some() {
                        self.emit_fellowship(actor)?;
                    }
                } else {
                    self.fellowships.panels.remove(&actor);
                }
                Ok(())
            }
        }
    }
    pub(in crate::kernel) fn queue_social_confirmation(
        &mut self,
        recipient: EntityId,
        confirmation: crate::social::SocialConfirmation,
        kind: u32,
        text: String,
    ) -> Result<u32, E> {
        if self.social.confirmations.contains_key(&recipient) {
            return Err(E::Busy);
        }
        if self.social.confirmations.len() >= self.social.capacity {
            return Err(E::Capacity);
        }
        let token = self
            .social
            .next_confirmation
            .checked_add(1)
            .ok_or(E::Overflow)?;
        let expires_tick = self.tick.checked_add(30 * 60).ok_or(E::Overflow)?;
        self.social.confirmations.insert(
            recipient,
            crate::social::PendingSocialConfirmation {
                token,
                expires_tick,
                confirmation,
            },
        );
        self.social.next_confirmation = token;
        self.social.events.push_back(SocialEvent::Confirmation {
            recipient,
            kind,
            token,
            text,
        });
        Ok(token)
    }
    pub(in crate::kernel) fn remove_fellowship_member(
        &mut self,
        actor: EntityId,
        dismissed: bool,
        now: u64,
    ) -> Result<(), E> {
        let before = self
            .fellowships
            .membership(actor)
            .ok_or(E::NotMember)?
            .clone();
        let successor = if before.leader() == actor && before.members().len() > 1 {
            let candidates: Vec<_> = before
                .members()
                .iter()
                .copied()
                .filter(|id| *id != actor)
                .collect();
            let root = self.social.random.as_ref().ok_or(E::Missing)?;
            let mut identity = [0; 16];
            identity[..8].copy_from_slice(&before.id.to_le_bytes());
            identity[8..].copy_from_slice(&before.revision().to_le_bytes());
            let mut stream = root
                .event_stream(identity, bace_random::Domain::Social)
                .map_err(|_| E::Invalid)?;
            Some(
                candidates[stream
                    .below(candidates.len() as u64)
                    .map_err(|_| E::Overflow)? as usize],
            )
        } else {
            None
        };
        let mut after = before.clone();
        after
            .remove(actor, dismissed, now, successor)
            .map_err(map_fellow_error)?;
        self.fellowships.replace(&before, after)?;
        self.fellowships.panels.remove(&actor);
        self.social.events.push_back(SocialEvent::FellowshipLeft {
            recipients: before.members().to_vec(),
            actor,
            dismissed,
            disbanded: false,
        });
        if let Some(member) = before.members().iter().copied().find(|id| *id != actor) {
            self.emit_fellowship(member)?;
        }
        Ok(())
    }
    pub(in crate::kernel) fn validate_fellow_projection(&self, actor: EntityId) -> Result<(), E> {
        self.social.directory.presence(actor).ok_or(E::Missing)?;
        self.characters.native_services(actor).ok_or(E::Missing)?;
        for kind in [
            bace_entity::EntityVital::Health,
            bace_entity::EntityVital::Stamina,
            bace_entity::EntityVital::Mana,
        ] {
            self.world.vital(actor, kind).map_err(|_| E::Missing)?;
        }
        Ok(())
    }
    pub(in crate::kernel) fn emit_fellowship(&mut self, actor: EntityId) -> Result<(), E> {
        if self.social.events.len() >= self.social.capacity {
            return Err(E::Capacity);
        }
        let group = self.fellowships.membership(actor).ok_or(E::NotMember)?;
        let members: Vec<_> = group
            .members()
            .iter()
            .map(|id| {
                let presence = self.social.directory.presence(*id).ok_or(E::Missing)?;
                let level = self
                    .characters
                    .native_services(*id)
                    .ok_or(E::Missing)?
                    .level;
                let mut maximum = [0; 3];
                let mut current = [0; 3];
                for (index, kind) in [
                    bace_entity::EntityVital::Health,
                    bace_entity::EntityVital::Stamina,
                    bace_entity::EntityVital::Mana,
                ]
                .into_iter()
                .enumerate()
                {
                    let vital = self.world.vital(*id, kind).map_err(|_| E::Missing)?;
                    maximum[index] = vital.maximum;
                    current[index] = vital.current;
                }
                Ok(FellowSnapshot {
                    actor: *id,
                    name: presence.identity.name.clone(),
                    level,
                    maximum,
                    current,
                })
            })
            .collect::<Result<_, E>>()?;
        let inputs: Vec<_> = members
            .iter()
            .map(|m| bace_fellowship::FellowRewardMember {
                actor: m.actor,
                level: m.level,
                xp_to_next_level: 0,
                indoor: false,
                landblock: 0,
                distance_2d: 0.0,
            })
            .collect();
        let sharing = group.sharing(&inputs, 50).map_err(map_fellow_error)?;
        let snapshot = FellowshipSnapshot {
            id: group.id,
            revision: group.revision(),
            name: group.name().into(),
            leader: group.leader(),
            members,
            share_xp: sharing.share,
            even_share: sharing.even,
            open: group.open(),
            locked: group.locked(),
            departed: group.departed().collect(),
            locks: Vec::new(),
        };
        self.social.events.push_back(SocialEvent::Fellowship {
            recipients: group.members().to_vec(),
            snapshot,
        });
        Ok(())
    }
}
fn map_fellow_error(error: FellowshipError) -> E {
    match error {
        FellowshipError::Invalid => E::Invalid,
        FellowshipError::Capacity => E::Full,
        FellowshipError::Duplicate => E::Duplicate,
        FellowshipError::NotLeader => E::NotLeader,
        FellowshipError::NotMember => E::NotMember,
        FellowshipError::Locked => E::Locked,
        FellowshipError::Overflow => E::Overflow,
    }
}
