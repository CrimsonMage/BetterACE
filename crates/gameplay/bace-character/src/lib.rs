//! Character creation, progression, attributes and vitals.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod creation;
mod progression;
mod tables;
mod training;
mod training_rules;

pub use creation::{
    CreatedSkill, CreationRejection, CreationRules, CreationRulesError, CreationSkillCosts,
    ValidatedAllocation, validate_attributes,
};
pub use progression::{CharacterProgression, ProgressionStateError, TraitProgress, TraitState};
pub use tables::{ProgressionTables, RankTable, RankTableError};
pub use training_rules::{SkillCosts, SkillRulesError, SkillTrainingRules, TrainingSetupError};
