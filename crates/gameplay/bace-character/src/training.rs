use std::sync::Arc;

use bace_gameplay_api::{
    ProgressionProjection, ProgressionTarget, SkillAdvancement, SkillTrainingChange,
    SkillTrainingRejection, TrainSkill, TraitDetails,
};

use crate::{
    CharacterProgression, SkillTrainingRules, TrainingSetupError, TraitProgress,
    progression::OwnedTrait, training_rules::TrainingState,
};

impl CharacterProgression {
    /// Attach explicitly loaded authoritative credits/prices/augmentation state.
    /// Failure returns the original aggregate, retaining any dirty revisions.
    /// The caller resolves augmentation skills from actual character state;
    /// affected training remains unsupported instead of silently omitting it.
    pub fn with_training(
        mut self,
        rules: Arc<SkillTrainingRules>,
        credits: u32,
        augmentation_skills: &[u32],
    ) -> Result<Self, (TrainingSetupError, Self)> {
        if self.training.is_some() {
            return Err((TrainingSetupError::AlreadyConfigured, self));
        }
        if self.tables.trained_skills.rank(0) != 0 || self.tables.specialized_skills.rank(0) != 0 {
            return Err((TrainingSetupError::UnsupportedZeroRankTable, self));
        }
        match TrainingState::new(rules, credits, augmentation_skills) {
            Ok(training) => {
                self.training = Some(training);
                Ok(self)
            }
            Err(error) => Err((error, self)),
        }
    }

    pub fn available_skill_credits(&self) -> Option<u32> {
        self.training.as_ref().map(|training| training.credits)
    }

    /// In-world HandleActionTrainSkill/TrainSkill behavior, without creation XP.
    /// All checks precede state mutation, insertion, credit spend and revision.
    pub fn train_skill(
        &mut self,
        request: TrainSkill,
    ) -> Result<SkillTrainingChange, SkillTrainingRejection> {
        let training = self
            .training
            .as_ref()
            .ok_or(SkillTrainingRejection::Unavailable)?;
        let cost = training
            .rules
            .get(request.skill)
            .ok_or(SkillTrainingRejection::UnknownSkill)?;
        if request.quoted_credits < 0 {
            return Err(SkillTrainingRejection::NegativeQuotedCost);
        }
        if request.quoted_credits != cost.trained_cost {
            return Err(SkillTrainingRejection::PriceMismatch);
        }
        let remaining = training
            .credits
            .checked_sub(cost.trained_cost as u32)
            .ok_or(SkillTrainingRejection::InsufficientCredits)?;
        let target = ProgressionTarget::Skill(request.skill);
        let existing = self.traits.get(&target).copied();
        // ACE GetCreatureSkill creates a new PropertiesSkill with Untrained and
        // CLR-zero fields. These are explicit new-record defaults, not missing
        // metadata guessed for an existing loaded skill.
        let mut entry = existing.unwrap_or(OwnedTrait {
            progress: TraitProgress {
                target,
                experience_spent: 0,
                advancement: SkillAdvancement::Untrained,
            },
            details: Some(TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            }),
        });
        if matches!(
            entry.progress.advancement,
            SkillAdvancement::Trained | SkillAdvancement::Specialized
        ) {
            return Err(SkillTrainingRejection::AlreadyTrained);
        }
        let Some(TraitDetails::Skill {
            resistance_at_last_check,
            last_used_time,
            ..
        }) = entry.details
        else {
            return Err(SkillTrainingRejection::MissingTraitDetails);
        };
        if existing.is_none() && self.traits.len() >= 256 {
            return Err(SkillTrainingRejection::TraitCapacity);
        }
        let before = self.projection(target).unwrap_or(ProgressionProjection {
            target,
            experience_spent: 0,
            ranks: 0,
            advancement: SkillAdvancement::Untrained,
            details: entry.details,
        });
        let augmented = training.augmentation_skills.contains(&request.skill);
        entry.progress.advancement = if augmented {
            SkillAdvancement::Specialized
        } else {
            SkillAdvancement::Trained
        };
        entry.progress.experience_spent = 0;
        entry.details = Some(TraitDetails::Skill {
            initial_level: if augmented { 10 } else { 0 },
            resistance_at_last_check,
            last_used_time,
        });
        self.commit_training(entry, before, remaining)
    }

    /// Server-only in-world specialization (ACE resetSkill=false). A device or
    /// quest owns its eligibility, item and durable-operation checks separately.
    /// No client action may invoke this merely by supplying a target skill ID.
    pub fn specialize_skill(
        &mut self,
        skill: u32,
    ) -> Result<SkillTrainingChange, SkillTrainingRejection> {
        let training = self
            .training
            .as_ref()
            .ok_or(SkillTrainingRejection::Unavailable)?;
        let cost = training
            .rules
            .get(skill)
            .ok_or(SkillTrainingRejection::UnknownSkill)?;
        let target = ProgressionTarget::Skill(skill);
        let mut entry = self
            .traits
            .get(&target)
            .copied()
            .ok_or(SkillTrainingRejection::NotTrained)?;
        if entry.progress.advancement != SkillAdvancement::Trained {
            return Err(SkillTrainingRejection::NotTrained);
        }
        let remaining = training
            .credits
            .checked_sub(cost.specialized_cost as u32)
            .ok_or(SkillTrainingRejection::InsufficientCredits)?;
        let Some(TraitDetails::Skill {
            resistance_at_last_check,
            last_used_time,
            ..
        }) = entry.details
        else {
            return Err(SkillTrainingRejection::MissingTraitDetails);
        };
        if crate::is_augmentation_skill(skill) {
            return Err(SkillTrainingRejection::InvalidSpecialization);
        }
        let total = self
            .specialized_credit_total()
            .map_err(|_| SkillTrainingRejection::SpecializationCap)?;
        let cap_cost = training
            .rules
            .cap_cost(skill)
            .ok_or(SkillTrainingRejection::UnknownSkill)?;
        if total.checked_add(cap_cost).is_none_or(|sum| sum > 70) {
            return Err(SkillTrainingRejection::SpecializationCap);
        }
        let before = self.projection(target).expect("existing trait");
        entry.progress.advancement = SkillAdvancement::Specialized;
        entry.details = Some(TraitDetails::Skill {
            initial_level: 10,
            resistance_at_last_check,
            last_used_time,
        });
        self.commit_training(entry, before, remaining)
    }

    fn commit_training(
        &mut self,
        entry: OwnedTrait,
        before: ProgressionProjection,
        remaining: u32,
    ) -> Result<SkillTrainingChange, SkillTrainingRejection> {
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(SkillTrainingRejection::RevisionExhausted)?;
        let target = entry.progress.target;
        self.traits.insert(target, entry);
        self.training.as_mut().expect("training validated").credits = remaining;
        self.revision = revision;
        Ok(SkillTrainingChange {
            before,
            after: self.projection(target).expect("committed trait"),
            available_skill_credits: remaining,
            revision,
        })
    }
}
