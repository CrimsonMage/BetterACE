//! ACE Player_Skills, SkillAlterationDevice and skill AugmentationDevice rules.
//! Pure proposals must be durably committed with device consumption by the owner.
use crate::CharacterProgression;
use bace_gameplay_api::{
    ProgressionProjection, ProgressionTarget, SkillAdvancement, SkillTrainingRejection,
    TraitDetails,
};

pub const LOCKED_SKILLS: [u32; 6] = [14, 15, 22, 24, 36, 40];
pub const AUGMENTATION_SKILLS: [u32; 5] = [18, 28, 29, 30, 40];
pub fn is_locked_skill(skill: u32) -> bool {
    LOCKED_SKILLS.contains(&skill)
}
pub fn is_augmentation_skill(skill: u32) -> bool {
    AUGMENTATION_SKILLS.contains(&skill)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillTransitionError {
    Training(SkillTrainingRejection),
    ExperienceOverflow,
    CreditOverflow,
    InvalidAugmentation,
    AlreadyAugmented,
    InsufficientExperience,
    WieldRequirement,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkillTransitionChange {
    pub before: ProgressionProjection,
    pub after: ProgressionProjection,
    pub available_experience: u64,
    pub available_skill_credits: u32,
    pub revision: u64,
    /// Durable augmentation flags remain in the player numeric properties.
    pub augmentation_added: bool,
}
impl From<SkillTrainingRejection> for SkillTransitionError {
    fn from(value: SkillTrainingRejection) -> Self {
        Self::Training(value)
    }
}
/// Prepared equipment requirements, converted from legacy skill IDs by the
/// inventory owner. All four requirement slots must be included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillWieldRequirement {
    RawSkill(u32),
    CurrentSkill(u32),
    Training {
        skill: u32,
        advancement: SkillAdvancement,
    },
}
pub fn lowering_blocked(
    skill: u32,
    class: SkillAdvancement,
    requirements: &[SkillWieldRequirement],
) -> bool {
    requirements.iter().any(|requirement| match *requirement {
        SkillWieldRequirement::RawSkill(id) | SkillWieldRequirement::CurrentSkill(id) => {
            id == skill
        }
        SkillWieldRequirement::Training {
            skill: id,
            advancement,
        } => id == skill && (advancement as u32) >= class as u32,
    })
}
impl CharacterProgression {
    pub fn augmented_skills(&self) -> impl Iterator<Item = u32> + '_ {
        self.training
            .iter()
            .flat_map(|state| state.augmentation_skills.iter().copied())
    }
    pub fn specialized_credit_total(&self) -> Result<u32, SkillTransitionError> {
        let training = self
            .training
            .as_ref()
            .ok_or(SkillTrainingRejection::Unavailable)?;
        self.traits.values().try_fold(0u32, |total, entry| {
            let ProgressionTarget::Skill(skill) = entry.progress.target else {
                return Ok(total);
            };
            if entry.progress.advancement != SkillAdvancement::Specialized
                || skill == 0
                || is_augmentation_skill(skill)
            {
                return Ok(total);
            }
            let cost = training
                .rules
                .cap_cost(skill)
                .ok_or(SkillTrainingRejection::UnknownSkill)?;
            total
                .checked_add(cost)
                .ok_or(SkillTransitionError::CreditOverflow)
        })
    }
    /// Temple lowering. Recheck equipment on confirmation before calling this.
    pub fn lower_skill(
        &mut self,
        skill: u32,
        wielded: &[SkillWieldRequirement],
    ) -> Result<SkillTransitionChange, SkillTransitionError> {
        let current = self
            .projection(ProgressionTarget::Skill(skill))
            .ok_or(SkillTrainingRejection::UnknownSkill)?;
        if lowering_blocked(skill, current.advancement, wielded) {
            return Err(SkillTransitionError::WieldRequirement);
        }
        self.refund_skill(skill, false)
    }
    /// Server/NPC full reset, distinct from the one-class temple lowering.
    /// Augmentation flags are retained; tinkering re-specializes on retraining.
    pub fn reset_skill(
        &mut self,
        skill: u32,
    ) -> Result<SkillTransitionChange, SkillTransitionError> {
        self.refund_skill(skill, true)
    }
    fn refund_skill(
        &mut self,
        skill: u32,
        full: bool,
    ) -> Result<SkillTransitionChange, SkillTransitionError> {
        let training = self
            .training
            .as_ref()
            .ok_or(SkillTrainingRejection::Unavailable)?;
        let cost = training
            .rules
            .get(skill)
            .ok_or(SkillTrainingRejection::UnknownSkill)?;
        let target = ProgressionTarget::Skill(skill);
        let before = self
            .projection(target)
            .ok_or(SkillTrainingRejection::UnknownSkill)?;
        if (before.advancement as u32) < SkillAdvancement::Trained as u32 {
            return Err(SkillTrainingRejection::NotTrained.into());
        }
        let Some(TraitDetails::Skill {
            initial_level,
            resistance_at_last_check,
            last_used_time,
        }) = before.details
        else {
            return Err(SkillTrainingRejection::MissingTraitDetails.into());
        };
        let augmented = training.augmentation_skills.contains(&skill);
        let locked = is_locked_skill(skill);
        let mut class = before.advancement;
        let mut init = initial_level;
        let mut refund = 0u32;
        if full {
            if class == SkillAdvancement::Specialized && !(augmented && locked) {
                class = SkillAdvancement::Trained;
                init = 0;
                if !augmented {
                    refund = cost.specialized_cost as u32;
                }
            }
            if !locked {
                class = SkillAdvancement::Untrained;
                init = 0;
                refund = refund
                    .checked_add(cost.trained_cost as u32)
                    .ok_or(SkillTransitionError::CreditOverflow)?;
            }
        } else if class == SkillAdvancement::Specialized {
            if !augmented {
                class = SkillAdvancement::Trained;
                init = 0;
                refund = cost.specialized_cost as u32;
            }
        } else if !locked {
            class = SkillAdvancement::Untrained;
            init = 0;
            refund = cost.trained_cost as u32;
        }
        let credits = training
            .credits
            .checked_add(refund)
            .filter(|c| *c <= i32::MAX as u32)
            .ok_or(SkillTransitionError::CreditOverflow)?;
        let xp = self
            .available_experience
            .checked_add(u64::from(before.experience_spent))
            .filter(|x| *x <= i64::MAX as u64)
            .ok_or(SkillTransitionError::ExperienceOverflow)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(SkillTrainingRejection::RevisionExhausted)?;
        let entry = self.traits.get_mut(&target).expect("validated skill");
        entry.progress.advancement = class;
        entry.progress.experience_spent = 0;
        entry.details = Some(TraitDetails::Skill {
            initial_level: init,
            resistance_at_last_check,
            last_used_time,
        });
        self.training.as_mut().expect("validated training").credits = credits;
        self.available_experience = xp;
        self.revision = revision;
        Ok(SkillTransitionChange {
            before,
            after: self.projection(target).expect("committed skill"),
            available_experience: xp,
            available_skill_credits: credits,
            revision,
            augmentation_added: false,
        })
    }
    /// Five one-stack specialization augmentations. The trusted content price
    /// and consumed augmentation device must enter the same durable operation.
    pub fn augment_skill(
        &mut self,
        skill: u32,
        experience_cost: u64,
    ) -> Result<SkillTransitionChange, SkillTransitionError> {
        if !is_augmentation_skill(skill) || experience_cost > i64::MAX as u64 {
            return Err(SkillTransitionError::InvalidAugmentation);
        }
        let training = self
            .training
            .as_ref()
            .ok_or(SkillTrainingRejection::Unavailable)?;
        if training.rules.get(skill).is_none() {
            return Err(SkillTrainingRejection::UnknownSkill.into());
        }
        if training.augmentation_skills.contains(&skill) {
            return Err(SkillTransitionError::AlreadyAugmented);
        }
        let target = ProgressionTarget::Skill(skill);
        let before = self
            .projection(target)
            .ok_or(SkillTrainingRejection::UnknownSkill)?;
        if before.advancement != SkillAdvancement::Trained {
            return Err(SkillTrainingRejection::NotTrained.into());
        }
        let Some(TraitDetails::Skill {
            resistance_at_last_check,
            last_used_time,
            ..
        }) = before.details
        else {
            return Err(SkillTrainingRejection::MissingTraitDetails.into());
        };
        let xp = self
            .available_experience
            .checked_sub(experience_cost)
            .ok_or(SkillTransitionError::InsufficientExperience)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(SkillTrainingRejection::RevisionExhausted)?;
        let credits = training.credits;
        let entry = self.traits.get_mut(&target).expect("validated skill");
        entry.progress.advancement = SkillAdvancement::Specialized;
        entry.details = Some(TraitDetails::Skill {
            initial_level: 10,
            resistance_at_last_check,
            last_used_time,
        });
        self.training
            .as_mut()
            .expect("validated training")
            .augmentation_skills
            .insert(skill);
        self.available_experience = xp;
        self.revision = revision;
        Ok(SkillTransitionChange {
            before,
            after: self.projection(target).expect("committed skill"),
            available_experience: xp,
            available_skill_credits: credits,
            revision,
            augmentation_added: true,
        })
    }
}
