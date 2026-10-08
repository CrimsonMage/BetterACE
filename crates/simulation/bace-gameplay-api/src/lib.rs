//! Typed commands, effects and subsystem contracts.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod action;
mod combat;
mod creation;
mod door;
mod progression;
mod training;
pub mod visibility;

pub use action::{ActionContext, ActionResult, CharacterBinding, SessionId};
pub use combat::{CombatChange, CombatOutcome, CombatRejection, CombatRequest};
pub use creation::{CreationAllocation, CreationAttributes};
pub use door::{DoorChange, DoorOutcome, DoorRejection, UseDoor};
pub use progression::{
    AttributeId, InvalidProgressionId, ProgressionActionRejection, ProgressionChange,
    ProgressionOutcome, ProgressionProjection, ProgressionRejection, ProgressionTarget,
    RaiseProgression, RankEffect, SkillAdvancement, TraitDetails, VitalId,
};
pub use training::{
    SkillTrainingActionRejection, SkillTrainingChange, SkillTrainingOutcome,
    SkillTrainingRejection, TrainSkill,
};
mod rares;
pub use rares::{CharacterRareState, RareAward, RareDecision, RareKillContext};
mod inventory_actions;
pub use inventory_actions::*;
mod housing;
pub use housing::*;
mod magic;
pub use magic::{CastChange, CastOutcome, CastRejection, CastRequest};
mod emotes;
pub use emotes::*;
mod loot;
pub use loot::GeneratedItemMutation;

mod ui;
pub use ui::{CharacterUi, UiError, UiRequest, UiShortcut};

mod enchantments;
pub use enchantments::EnchantmentProjection;

pub mod weapon_combat;

mod magic_origin;
pub use magic_origin::{CastOrigin, ServerCastOutcome};

pub mod generators;
pub use generators::*;

pub mod corpse_consent;
pub mod social;

pub mod staff;

pub mod item_experience;

pub mod experience;

pub mod locomotion;

pub mod staff_gags;

pub mod selection;

pub mod physical_procs;
