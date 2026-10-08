//! Skill proposals reserve the character until the exact durable receipt arrives.
use super::*;
use bace_character::{
    SkillProposal, SkillTransitionChange, SkillTransitionError, SkillWieldRequirement,
};
use bace_gameplay_api::TrainSkill;
#[derive(Clone, Debug)]
pub enum SkillIntent {
    Train(TrainSkill),
    Specialize(u32),
    Lower {
        skill: u32,
        wielded: Vec<SkillWieldRequirement>,
    },
    Reset(u32),
    Augment {
        skill: u32,
        experience_cost: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkillTicket {
    pub operation: u64,
    pub context: ActionContext,
    pub change: SkillTransitionChange,
    pub expected_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillActionError {
    Authority(ProgressionActionRejection),
    Domain(SkillTransitionError),
    Busy,
    Capacity,
    Receipt,
    Profile,
}
pub(super) struct PendingSkill {
    ticket: SkillTicket,
    proposal: SkillProposal,
}
impl Characters {
    pub(crate) fn pending_skill_operation(&self, actor: EntityId) -> Option<u64> {
        self.entries
            .get(&actor)?
            .skill
            .as_ref()
            .map(|s| s.ticket.operation)
    }
    pub(crate) fn propose_skill(
        &mut self,
        context: ActionContext,
        intent: SkillIntent,
        exists: bool,
    ) -> Result<SkillTicket, SkillActionError> {
        if self.reserved(context.actor) {
            return Err(SkillActionError::Busy);
        }
        self.authorize(context, exists)
            .map_err(SkillActionError::Authority)?;
        let operation = self
            .next_skill
            .checked_add(1)
            .ok_or(SkillActionError::Capacity)?;
        let entry = self
            .entries
            .get_mut(&context.actor)
            .ok_or(SkillActionError::Receipt)?;
        let proposal = match intent {
            SkillIntent::Train(v) => entry.progression.propose_train_skill(v),
            SkillIntent::Specialize(v) => entry.progression.propose_specialize_skill(v),
            SkillIntent::Lower { skill, wielded } => {
                entry.progression.propose_lower_skill(skill, &wielded)
            }
            SkillIntent::Reset(v) => entry.progression.propose_reset_skill(v),
            SkillIntent::Augment {
                skill,
                experience_cost,
            } => entry
                .progression
                .propose_augment_skill(skill, experience_cost),
        }
        .map_err(SkillActionError::Domain)?;
        let ticket = SkillTicket {
            operation,
            context,
            change: proposal.change(),
            expected_revision: proposal.expected_revision(),
        };
        entry.skill = Some(PendingSkill { ticket, proposal });
        self.next_skill = operation;
        Ok(ticket)
    }
    pub(crate) fn proposed_skill_state(
        &self,
        ticket: SkillTicket,
    ) -> Result<&CharacterProgression, SkillActionError> {
        self.validate_skill_ticket(ticket)?;
        Ok(self.entries[&ticket.context.actor]
            .skill
            .as_ref()
            .ok_or(SkillActionError::Receipt)?
            .proposal
            .proposed_state())
    }
    pub(crate) fn validate_skill_ticket(
        &self,
        ticket: SkillTicket,
    ) -> Result<(), SkillActionError> {
        let entry = self
            .entries
            .get(&ticket.context.actor)
            .ok_or(SkillActionError::Receipt)?;
        let pending = entry
            .skill
            .as_ref()
            .filter(|pending| pending.ticket == ticket)
            .ok_or(SkillActionError::Receipt)?;
        let before = pending.proposal.expected_state();
        if entry.progression.revision() != before.revision()
            || entry.progression.available_experience() != before.available_experience()
            || entry.progression.available_skill_credits() != before.available_skill_credits()
            || !entry.progression.traits().eq(before.traits())
            || !entry
                .progression
                .augmented_skills()
                .eq(before.augmented_skills())
        {
            return Err(SkillActionError::Receipt);
        }
        Ok(())
    }
    pub(crate) fn commit_skill(
        &mut self,
        ticket: SkillTicket,
    ) -> Result<SkillTransitionChange, SkillActionError> {
        let entry = self
            .entries
            .get_mut(&ticket.context.actor)
            .ok_or(SkillActionError::Receipt)?;
        if entry.skill.as_ref().is_none_or(|p| p.ticket != ticket) {
            return Err(SkillActionError::Receipt);
        }
        let pending = entry.skill.take().ok_or(SkillActionError::Receipt)?;
        match entry.progression.adopt_skill_proposal(pending.proposal) {
            Ok(change) => Ok(change),
            Err(stale) => {
                entry.skill = Some(PendingSkill {
                    ticket,
                    proposal: stale.0,
                });
                Err(SkillActionError::Receipt)
            }
        }
    }
    pub(crate) fn reject_skill(&mut self, ticket: SkillTicket) -> Result<(), SkillActionError> {
        let entry = self
            .entries
            .get_mut(&ticket.context.actor)
            .ok_or(SkillActionError::Receipt)?;
        if entry.skill.as_ref().is_none_or(|p| p.ticket != ticket) {
            return Err(SkillActionError::Receipt);
        }
        entry.skill = None;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SkillStage {
    Proposed(SkillTicket),
    Committed(SkillTicket),
    RolledBack(SkillTicket),
}
pub type SkillOutcome = bace_gameplay_api::ActionResult<SkillStage, SkillActionError>;
pub type UiOutcome = bace_gameplay_api::ActionResult<u64, bace_gameplay_api::UiError>;
