//! Source gag playtime and durable proposal adoption on the existing owner.
use super::*;
use crate::social_gags::{GagRecovery, PendingGag};
use bace_gameplay_api::{
    social::{AcceptedChat, ChatChannel, ChatDelivery, SocialEvent, SocialIdentity},
    staff::{StaffError as E, StaffEvent},
    staff_gags::StaffGagProposal,
};
impl Kernel {
    pub fn gag_snapshot(&self, actor: EntityId) -> Option<GagRecovery> {
        self.social_gags.states.get(&actor).copied()
    }
    pub(super) fn gag_pending(&self, actor: EntityId) -> bool {
        self.social_gags
            .pending
            .values()
            .any(|p| p.proposal.context.actor == actor || p.proposal.target.character == actor)
    }
    pub fn register_gag(&mut self, actor: EntityId, state: GagRecovery) -> Result<(), E> {
        state.validate().map_err(|_| E::Invalid)?;
        if self.social_gags.states.len() >= 4096
            || self.social_gags.states.contains_key(&actor)
            || self.characters.get(actor).is_none()
        {
            return Err(E::Busy);
        }
        let presence = self
            .social
            .directory
            .presence(actor)
            .ok_or(E::MissingTarget)?;
        if presence.gagged != state.state.active {
            return Err(E::Invalid);
        }
        self.social_gags.states.insert(actor, state);
        Ok(())
    }
    pub(super) fn queue_gag_heartbeat(&mut self, actor: EntityId, interval: f64) {
        if let Some(state) = self.social_gags.states.get_mut(&actor) {
            // No elapsed online heartbeat is lost under output pressure. Once
            // the retained duration reaches expiry, further beats are immaterial.
            state.pending_interval = Some(
                (state.pending_interval.unwrap_or(0.) + interval)
                    .min(state.state.remaining.max(interval)),
            );
        }
    }

    pub(super) fn step_social_gags(&mut self) -> Result<(), SimulationError> {
        let mut ids = std::mem::take(&mut self.social_gags.scratch);
        ids.clear();
        ids.extend(
            self.social_gags
                .states
                .iter()
                .filter_map(|(id, s)| s.pending_interval.map(|_| *id)),
        );
        for actor in &ids {
            if self.characters.reserved(*actor) {
                continue;
            }
            let old = self.social_gags.states[actor];
            let change = old
                .state
                .heartbeat(old.pending_interval.expect("pending beat"))
                .map_err(|_| SimulationError::InvalidCommand)?;
            let count = usize::from(change.notices.suspended) * 2
                + usize::from(change.notices.restored) * 2;
            if self.social.events.len() + count > self.social.capacity {
                continue;
            }
            if change.durable_changed()
                && !self
                    .characters
                    .touch_auxiliary(*actor)
                    .map_err(|_| SimulationError::AuxiliaryRevision)?
            {
                continue;
            }
            let mut presence = self
                .social
                .directory
                .presence(*actor)
                .ok_or(SimulationError::InvalidCommand)?
                .clone();
            presence.gagged = change.after.active;
            self.social
                .directory
                .update_presence(presence)
                .map_err(|_| SimulationError::InvalidCommand)?;
            self.social_gags.states.insert(
                *actor,
                GagRecovery {
                    state: change.after,
                    pending_interval: None,
                },
            );
            for text in [
                change
                    .notices
                    .suspended
                    .then_some("Your chat privileges have been suspended."),
                change
                    .notices
                    .restored
                    .then_some("Your chat privileges have been restored."),
            ]
            .into_iter()
            .flatten()
            {
                self.social.events.push_back(SocialEvent::Transient {
                    recipient: *actor,
                    text: text.into(),
                });
                self.social.events.push_back(SocialEvent::System {
                    recipient: *actor,
                    text: text.into(),
                    chat_type: 20,
                });
            }
        }
        ids.clear();
        self.social_gags.scratch = ids;
        Ok(())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "exact authenticated source command fields"
    )]
    pub fn prepare_staff_gag(
        &mut self,
        context: ActionContext,
        operation: u64,
        target: SocialIdentity,
        requested_name: String,
        enabled: bool,
        unix_seconds: f64,
        sudo: bool,
    ) -> Result<(), E> {
        if operation == 0
            || target.character.0 == 0
            || target.account.0 == 0
            || requested_name.is_empty()
            || requested_name.len() > 100
            || requested_name.contains('\0')
            || target.name.is_empty()
            || target.name.len() > 100
            || !unix_seconds.is_finite()
            || unix_seconds < 0.
            || self.social_gags.pending.contains_key(&operation)
            || self.social_gags.pending.len() >= 256
        {
            return Err(E::Invalid);
        }
        let issuer_name = self
            .social
            .directory
            .presence(context.actor)
            .ok_or(E::MissingTarget)?
            .identity
            .name
            .clone();
        let online = self
            .social
            .directory
            .presence(target.character)
            .is_some_and(|p| p.online);
        // Source heartbeat work accepted before this command must be applied to
        // the prior gag, never debited from the newly reset five-minute duration.
        if online
            && self
                .social_gags
                .states
                .get(&target.character)
                .is_some_and(|s| s.pending_interval.is_some())
        {
            return Err(E::Busy);
        }
        let change = if online {
            if self
                .social
                .directory
                .presence(target.character)
                .is_none_or(|p| p.identity != target)
                || self.characters.reserved(target.character)
                || self.magic.registry_reserved(target.character)
            {
                return Err(E::Busy);
            }
            Some(
                self.social_gags
                    .states
                    .get(&target.character)
                    .ok_or(E::MissingTarget)?
                    .state
                    .change(enabled, unix_seconds)
                    .map_err(|_| E::Invalid)?,
            )
        } else {
            None
        };
        self.authorize_staff(context, 2, sudo)?;
        self.magic
            .prepare_registry_time(self.tick as f64 / 30.)
            .map_err(|_| E::Busy)?;
        self.sync_registry_revisions().map_err(|_| E::Overflow)?;
        let before_revision = if change.is_some() {
            let revision = self
                .characters
                .get(target.character)
                .ok_or(E::MissingTarget)?
                .revision();
            self.magic
                .reserve_registry(target.character, true, self.tick as f64 / 30.)
                .map_err(|_| E::Busy)?;
            if self
                .characters
                .reserve_gag(target.character, operation, revision)
                .is_err()
            {
                self.magic
                    .reserve_registry(target.character, false, self.tick as f64 / 30.)
                    .map_err(|_| E::Busy)?;
                return Err(E::Busy);
            }
            Some(revision)
        } else {
            None
        };
        let proposal = StaffGagProposal {
            operation,
            context,
            target,
            requested_name,
            before_revision,
            enabled,
            unix_seconds,
        };
        self.social_gags.pending.insert(
            operation,
            PendingGag {
                proposal: proposal.clone(),
                change,
                issuer_name,
            },
        );
        self.staff.push(StaffEvent::GagProposal(proposal));
        Ok(())
    }
    pub(super) fn gag_commit_room(&self) -> bool {
        self.social.events.len() + 2 <= self.social.capacity && self.social.sequence < u64::MAX
    }
    pub fn complete_staff_gag(
        &mut self,
        proposal: &StaffGagProposal,
        commit: bool,
    ) -> Result<(), E> {
        let pending = self
            .social_gags
            .pending
            .get(&proposal.operation)
            .filter(|p| p.proposal == *proposal)
            .ok_or(E::Stale)?;
        if (commit && !self.gag_commit_room())
            || (!commit && self.social.events.len() >= self.social.capacity)
        {
            return Err(E::Capacity);
        }
        let actor = proposal.target.character;
        let issuer_name = pending.issuer_name.clone();
        let now = if commit {
            Some(self.social_now().map_err(|_| E::Overflow)?)
        } else {
            None
        };
        if let Some(change) = pending.change {
            let state = self
                .social_gags
                .states
                .get(&actor)
                .ok_or(E::MissingTarget)?;
            if state.state != change.before
                || self.characters.gag_operation(actor) != Some(proposal.operation)
                || self.characters.get(actor).is_none_or(|p| {
                    Some(p.revision()) != proposal.before_revision
                        || commit && p.revision() == u64::MAX
                })
                || self
                    .social
                    .directory
                    .presence(actor)
                    .is_none_or(|p| p.identity != proposal.target)
            {
                return Err(E::Stale);
            }
            self.magic
                .reserve_registry(actor, false, self.tick as f64 / 30.)
                .map_err(|_| E::Busy)?;
            self.characters
                .finish_gag(
                    actor,
                    proposal.operation,
                    proposal.before_revision.ok_or(E::Invalid)?,
                    commit,
                )
                .map_err(|_| E::Stale)?;
            if commit {
                self.social_gags
                    .states
                    .get_mut(&actor)
                    .expect("checked gag")
                    .state = change.after;
                let mut presence = self
                    .social
                    .directory
                    .presence(actor)
                    .ok_or(E::MissingTarget)?
                    .clone();
                presence.gagged = change.after.active;
                self.social
                    .directory
                    .update_presence(presence)
                    .map_err(|_| E::Stale)?;
            }
        }
        if commit {
            let suffix = if proposal.enabled {
                "gagged"
            } else {
                "ungagged"
            };
            let duration = if proposal.enabled {
                " for five minutes"
            } else {
                ""
            };
            let text = format!(
                "{issuer_name} has {suffix} {}{duration}.",
                proposal.target.name
            );
            let recipients = self
                .social
                .directory
                .online()
                .filter(|id| {
                    self.social
                        .directory
                        .preferences(*id)
                        .is_some_and(|p| p.channels.contains(&4))
                })
                .take(4096)
                .collect();
            let now = now.expect("committed gag clock preflight");
            self.social.sequence += 1;
            self.social.events.push_back(SocialEvent::Chat {
                accepted: AcceptedChat {
                    sequence: self.social.sequence,
                    unix_seconds: now,
                    sender: proposal.context.actor,
                    sender_name: issuer_name,
                    channel: ChatChannel::Audit,
                    text,
                },
                wire: ChatDelivery::LegacyChannel(4),
                recipients,
            });
            self.social.events.push_back(SocialEvent::System {
                recipient: proposal.context.actor,
                text: format!("{} has been {suffix}{duration}.", proposal.requested_name),
                chat_type: 20,
            });
        }
        if !commit {
            let verb = if proposal.enabled { "gag" } else { "ungag" };
            self.social.events.push_back(SocialEvent::System {
                recipient: proposal.context.actor,
                text: format!(
                    "Unable to {verb} a character named {}, check the name and re-try the command.",
                    proposal.requested_name
                ),
                chat_type: 20,
            });
        }
        self.social_gags.pending.remove(&proposal.operation);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
