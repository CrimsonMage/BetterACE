//! Publish only accepted, durable XP state; queue capacity participates in commit admission.
use super::Kernel;
use bace_gameplay_api::{
    experience::{ExperienceEvent, ExperienceState},
    social::SocialError,
};
impl Kernel {
    pub fn peek_experience_event(&self) -> Option<&ExperienceEvent> {
        self.allegiances.experience_events.front()
    }
    pub fn take_experience_event(&mut self) -> Option<ExperienceEvent> {
        self.allegiances.experience_events.pop_front()
    }
    pub(super) fn validate_experience_events(
        &self,
        ticket: &crate::AllegianceTicket,
    ) -> Result<(), SocialError> {
        let count = ticket
            .player_changes
            .len()
            .checked_add(ticket.quest_messages.len())
            .ok_or(SocialError::Overflow)?;
        if self
            .allegiances
            .experience_events
            .len()
            .saturating_add(count)
            > self.allegiances.capacity
        {
            return Err(SocialError::Capacity);
        }
        if !ticket.player_changes.is_empty() && self.allegiances.level_table.is_none() {
            return Err(SocialError::Missing);
        }
        Ok(())
    }
    pub(super) fn emit_experience_events(&mut self, ticket: &crate::AllegianceTicket) {
        let Some(table) = &self.allegiances.level_table else {
            return;
        };
        for (actor, change) in &ticket.player_changes {
            let before = ExperienceState {
                total: change.services.before.total_experience,
                available: change.experience.before_available,
                level: change.services.before.level,
                available_skill_credits: change.before_skill_credits.unwrap_or(0),
            };
            let after = ExperienceState {
                total: change.services.after.total_experience,
                available: change.experience.after_available,
                level: change.services.after.level,
                available_skill_credits: change.after_skill_credits.unwrap_or(0),
            };
            self.allegiances
                .experience_events
                .push_back(ExperienceEvent {
                    actor: *actor,
                    before,
                    after,
                    update_properties: true,
                    maximum_level: table.maximum_level(),
                    quest_amount: None,
                    next_credit_level: (after.level > before.level
                        && change.earned_skill_credits == 0
                        && after.level < table.maximum_level())
                    .then(|| table.next_skill_credit_level(after.level)),
                    vitals: ticket
                        .vitals
                        .iter()
                        .filter(|v| v.actor == *actor)
                        .map(|v| {
                            (
                                match v.vital {
                                    bace_entity::EntityVital::Health => 2,
                                    bace_entity::EntityVital::Stamina => 4,
                                    bace_entity::EntityVital::Mana => 6,
                                },
                                v.after,
                            )
                        })
                        .collect(),
                });
        }
        for &(actor, amount) in &ticket.quest_messages {
            let change = ticket
                .player_changes
                .iter()
                .find(|(id, _)| *id == actor)
                .expect("source earner in accepted reward");
            let state = ExperienceState {
                total: change.1.services.after.total_experience,
                available: change.1.experience.after_available,
                level: change.1.services.after.level,
                available_skill_credits: change.1.after_skill_credits.unwrap_or(0),
            };
            self.allegiances
                .experience_events
                .push_back(ExperienceEvent {
                    actor,
                    before: state,
                    after: state,
                    update_properties: false,
                    maximum_level: table.maximum_level(),
                    quest_amount: Some(amount),
                    next_credit_level: None,
                    vitals: Vec::new(),
                });
        }
    }
}
