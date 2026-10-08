//! Pinned DeveloperCommands.HandleGrantXp, receipt-gated through the existing XP owner.
use super::*;
use bace_gameplay_api::{
    social::{
        AcceptedChat, ChatChannel, ChatDelivery, EarnedExperience, RewardSharing, RewardXpKind,
        SocialError, SocialEvent,
    },
    staff::{StaffError as E, StaffEvent},
};
impl Kernel {
    pub fn staff_grant_experience(
        &mut self,
        context: ActionContext,
        target: EntityId,
        amount: u64,
        token: u64,
        sudo: bool,
    ) -> Result<(), E> {
        if amount == 0 || amount > i64::MAX as u64 || token == 0 {
            return Err(E::Invalid);
        }
        if self.social.capacity < 3 {
            return Err(E::Capacity);
        }
        self.authorize_staff(context, 4, sudo)?;
        let issuer_name = self
            .social
            .directory
            .presence(context.actor)
            .filter(|p| p.online)
            .ok_or(E::NotBound)?
            .identity
            .name
            .clone();
        let target_name = self
            .social
            .directory
            .presence(target)
            .filter(|p| p.online)
            .ok_or(E::MissingTarget)?
            .identity
            .name
            .clone();
        if self.staff.rewards.len() >= self.staff.capacity() {
            return Err(E::Capacity);
        }
        let ticket = self
            .prepare_shared_experience(EarnedExperience {
                source: target,
                amount,
                kind: RewardXpKind::Admin,
                sharing: RewardSharing {
                    fellowship: false,
                    allegiance: false,
                },
            })
            .map_err(|e| match e {
                SocialError::Busy => E::Busy,
                SocialError::Capacity => E::Capacity,
                _ => E::Invalid,
            })?;
        self.staff.rewards.insert(
            ticket.operation,
            crate::staff::PendingStaffReward {
                context,
                token,
                target,
                amount,
                issuer_name,
                target_name,
            },
        );
        Ok(())
    }
    pub(super) fn validate_staff_reward(
        &self,
        ticket: &crate::AllegianceTicket,
        success: bool,
    ) -> Result<(), SocialError> {
        let Some(pending) = self.staff.rewards.get(&ticket.operation) else {
            return Ok(());
        };
        if pending.target != ticket.actor {
            return Err(SocialError::Stale);
        }
        if !self.staff.room(1)
            || success && self.social.events.len().saturating_add(3) > self.social.capacity
        {
            return Err(SocialError::Capacity);
        }
        if success {
            self.social_now()?;
            self.social
                .sequence
                .checked_add(1)
                .ok_or(SocialError::Overflow)?;
        }
        Ok(())
    }
    pub(super) fn complete_staff_reward(
        &mut self,
        ticket: &crate::AllegianceTicket,
        success: bool,
    ) {
        let Some(pending) = self.staff.rewards.remove(&ticket.operation) else {
            return;
        };
        if success {
            let amount = grouped(pending.amount);
            self.social.events.push_back(SocialEvent::System {
                recipient: pending.context.actor,
                text: format!("{amount} experience granted."),
                chat_type: 13,
            });
            let recipients = self
                .social
                .directory
                .online()
                .filter(|actor| {
                    self.social
                        .directory
                        .preferences(*actor)
                        .is_some_and(|p| p.channels.contains(&4))
                })
                .collect();
            self.social.sequence += 1;
            let accepted = AcceptedChat {
                sequence: self.social.sequence,
                unix_seconds: self.social_now().expect("preflight audit clock"),
                sender: pending.context.actor,
                sender_name: pending.issuer_name.clone(),
                channel: ChatChannel::Audit,
                text: format!(
                    "{} granted {amount} experience to {}.",
                    pending.issuer_name, pending.target_name
                ),
            };
            self.social.events.push_back(SocialEvent::Chat {
                accepted,
                wire: ChatDelivery::LegacyChannel(4),
                recipients,
            });
        }
        self.staff.push(StaffEvent::Outcome {
            token: pending.token,
            actor: Some(pending.context.actor),
            result: if success { Ok(()) } else { Err(E::Stale) },
        });
    }
}
fn grouped(value: u64) -> String {
    let s = value.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}
