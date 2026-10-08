//! Immutable pending valuable operations. A proposal is not a second live actor:
//! its post-state has no public mutation access and cannot execute simulation.
use crate::{
    CharacterProgression, SkillTransitionChange, SkillTransitionError, SkillWieldRequirement,
};
use bace_gameplay_api::{SkillTrainingChange, SkillTrainingRejection, TrainSkill};

#[derive(Debug)]
pub struct SkillProposal {
    before: CharacterProgression,
    after: CharacterProgression,
    change: SkillTransitionChange,
}
impl SkillProposal {
    pub fn change(&self) -> SkillTransitionChange {
        self.change
    }
    /// Read-only proposed state for a frozen save adapter. Do not publish before
    /// durable success. Device ownership/operation IDs are adapter responsibilities.
    pub fn proposed_state(&self) -> &CharacterProgression {
        &self.after
    }
    pub fn expected_revision(&self) -> u64 {
        self.before.revision
    }
    pub fn expected_state(&self) -> &CharacterProgression {
        &self.before
    }
}
#[derive(Debug)]
pub struct StaleSkillProposal(pub SkillProposal);
impl CharacterProgression {
    pub fn propose_train_skill(
        &self,
        request: TrainSkill,
    ) -> Result<SkillProposal, SkillTransitionError> {
        self.propose(|state| {
            let c = state.train_skill(request)?;
            Ok(state.training_change(c))
        })
    }
    pub fn propose_specialize_skill(
        &self,
        skill: u32,
    ) -> Result<SkillProposal, SkillTransitionError> {
        self.propose(|state| {
            let c = state.specialize_skill(skill)?;
            Ok(state.training_change(c))
        })
    }
    pub fn propose_lower_skill(
        &self,
        skill: u32,
        wielded: &[SkillWieldRequirement],
    ) -> Result<SkillProposal, SkillTransitionError> {
        self.propose(|state| state.lower_skill(skill, wielded))
    }
    pub fn propose_reset_skill(&self, skill: u32) -> Result<SkillProposal, SkillTransitionError> {
        self.propose(|state| state.reset_skill(skill))
    }
    pub fn propose_augment_skill(
        &self,
        skill: u32,
        experience_cost: u64,
    ) -> Result<SkillProposal, SkillTransitionError> {
        self.propose(|state| state.augment_skill(skill, experience_cost))
    }
    /// Call only after the exact proposal's database commit is confirmed. A
    /// rejection returns ownership of the pending proposal for recovery.
    pub fn adopt_skill_proposal(
        &mut self,
        proposal: SkillProposal,
    ) -> Result<SkillTransitionChange, Box<StaleSkillProposal>> {
        let before = &proposal.before;
        if self.revision != before.revision
            || self.available_experience != before.available_experience
            || self.traits != before.traits
            || self.available_skill_credits() != before.available_skill_credits()
            || !self.augmented_skills().eq(before.augmented_skills())
            || self.luminance != before.luminance
            || !std::sync::Arc::ptr_eq(&self.tables, &before.tables)
        {
            return Err(Box::new(StaleSkillProposal(proposal)));
        }
        let change = proposal.change;
        *self = proposal.after;
        Ok(change)
    }
    fn training_change(&self, change: SkillTrainingChange) -> SkillTransitionChange {
        SkillTransitionChange {
            before: change.before,
            after: change.after,
            available_experience: self.available_experience,
            available_skill_credits: change.available_skill_credits,
            revision: change.revision,
            augmentation_added: false,
        }
    }
    fn propose(
        &self,
        apply: impl FnOnce(&mut Self) -> Result<SkillTransitionChange, SkillTransitionError>,
    ) -> Result<SkillProposal, SkillTransitionError> {
        if self.training.is_none() {
            return Err(SkillTrainingRejection::Unavailable.into());
        }
        let mut after = self.pending_copy();
        let change = apply(&mut after)?;
        Ok(SkillProposal {
            before: self.pending_copy(),
            after,
            change,
        })
    }
    pub(crate) fn pending_copy(&self) -> Self {
        Self {
            traits: self.traits.clone(),
            tables: self.tables.clone(),
            available_experience: self.available_experience,
            revision: self.revision,
            training: self.training.clone(),
            luminance: self.luminance,
        }
    }
}
