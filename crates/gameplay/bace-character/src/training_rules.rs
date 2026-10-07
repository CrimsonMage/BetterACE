use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Base DAT prices used after creation. Heritage creation overrides do not apply
/// to HandleActionTrainSkill and must not be substituted for these values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillCosts {
    pub skill: u32,
    pub trained_cost: i32,
    /// Incremental upgrade price: DAT total specialized cost minus trained cost.
    pub specialized_cost: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillRulesError {
    TooManySkills,
    NegativeCost,
    DuplicateSkill,
}

#[derive(Debug)]
pub struct SkillTrainingRules {
    costs: BTreeMap<u32, SkillCosts>,
}
impl SkillTrainingRules {
    pub fn new(costs: &[SkillCosts]) -> Result<Self, SkillRulesError> {
        if costs.len() > 256 {
            return Err(SkillRulesError::TooManySkills);
        }
        let mut entries = BTreeMap::new();
        for cost in costs {
            if cost.trained_cost < 0 || cost.specialized_cost < 0 {
                return Err(SkillRulesError::NegativeCost);
            }
            if entries.insert(cost.skill, *cost).is_some() {
                return Err(SkillRulesError::DuplicateSkill);
            }
        }
        Ok(Self { costs: entries })
    }
    pub(crate) fn get(&self, skill: u32) -> Option<SkillCosts> {
        self.costs.get(&skill).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainingSetupError {
    AlreadyConfigured,
    InvalidCredits,
    TooManyAugmentations,
    UnknownAugmentationSkill,
    DuplicateAugmentationSkill,
    UnsupportedZeroRankTable,
}

#[derive(Debug)]
pub(crate) struct TrainingState {
    pub(crate) rules: Arc<SkillTrainingRules>,
    pub(crate) credits: u32,
    pub(crate) augmentation_skills: BTreeSet<u32>,
}
impl TrainingState {
    pub(crate) fn new(
        rules: Arc<SkillTrainingRules>,
        credits: u32,
        augmentation_skills: &[u32],
    ) -> Result<Self, TrainingSetupError> {
        if credits > i32::MAX as u32 {
            return Err(TrainingSetupError::InvalidCredits);
        }
        if augmentation_skills.len() > 256 {
            return Err(TrainingSetupError::TooManyAugmentations);
        }
        let mut skills = BTreeSet::new();
        for skill in augmentation_skills {
            if rules.get(*skill).is_none() {
                return Err(TrainingSetupError::UnknownAugmentationSkill);
            }
            if !skills.insert(*skill) {
                return Err(TrainingSetupError::DuplicateAugmentationSkill);
            }
        }
        Ok(Self {
            rules,
            credits,
            augmentation_skills: skills,
        })
    }
}
