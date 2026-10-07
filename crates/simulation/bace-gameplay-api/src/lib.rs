//! Typed commands, effects and subsystem contracts.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod action;
mod creation;
mod progression;
mod training;

pub use action::{ActionContext, ActionResult, CharacterBinding, SessionId};
pub use creation::{CreationAllocation, CreationAttributes};
pub use progression::{
    AttributeId, InvalidProgressionId, ProgressionActionRejection, ProgressionChange,
    ProgressionOutcome, ProgressionProjection, ProgressionRejection, ProgressionTarget,
    RaiseProgression, SkillAdvancement, TraitDetails, VitalId,
};
pub use training::{
    SkillTrainingActionRejection, SkillTrainingChange, SkillTrainingOutcome,
    SkillTrainingRejection, TrainSkill,
};
