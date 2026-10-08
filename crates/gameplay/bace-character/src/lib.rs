//! Character creation, progression, attributes and vitals.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod creation;
mod factory;
mod factory_types;
mod progression;
mod reward;
mod tables;
mod training;
mod training_rules;

pub use creation::{
    CreatedSkill, CreationRejection, CreationRules, CreationRulesError, CreationSkillCosts,
    ValidatedAllocation, validate_attributes,
};
pub use factory::prepare_character;
pub use factory_types::*;
pub use progression::{CharacterProgression, ProgressionStateError, TraitProgress, TraitState};
pub use reward::{ExperienceCredit, RewardCreditError};
pub use tables::{ProgressionTables, RankTable, RankTableError};
pub use training_rules::{SkillCosts, SkillRulesError, SkillTrainingRules, TrainingSetupError};

mod aetheria;
mod attribute_transfer;
pub use attribute_transfer::{AttributeTransferError, AttributeTransferProposal};
mod luminance;
pub use aetheria::{
    AetheriaError, ItemExperience, ItemExperienceChange, ItemExperienceStyle, aetheria_proc,
    aetheria_proc_rate, aetheria_surge_self_target,
};
pub use luminance::{LuminanceCredit, LuminanceError, LuminanceModifiers, LuminanceState};

mod skill_transitions;
pub use skill_transitions::*;
mod skill_values;
pub use skill_values::*;
mod names;
pub use names::*;

mod skill_proposal;
pub mod ui;
pub use skill_proposal::{SkillProposal, StaleSkillProposal};

mod locomotion;
pub use locomotion::{
    Encumbrance, JumpInput, JumpProposal, LocomotionError, RunInput, encumbrance, jump_proposal,
    run_rate,
};

mod npc_services;
pub use npc_services::TrainingCreditChange;
pub use npc_services::{
    CharacterServiceChange, CharacterServiceError, CharacterServiceState, level_proportional_xp,
};

pub use npc_services::{CharacterLevelTable, EarnedExperienceChange};

mod vitae;
pub use vitae::{VitaeError, VitaeXpChange, inflict_vitae, vitae_experience, vitae_threshold};

mod read_snapshot;
pub use read_snapshot::ProgressionSnapshot;

mod vital_values;
pub use vital_values::{
    VitalValueInputs, VitalValues, project_attribute_value, project_vital_values,
};

mod proficiency;
pub use proficiency::{ProficiencyChange, ProficiencyUse};
