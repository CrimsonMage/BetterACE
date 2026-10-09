//! Bounded authoritative replication sequencing and observer knowledge.
//! Complete world/object projections remain a separate integration gate.
mod sequences;
mod spatial_visibility;
mod visibility;
pub use sequences::{ReplicationError, SequenceKind, Sequences};
pub use spatial_visibility::{SpatialVisibility, SpatialVisibilityError, VisibilityDelta};
pub use visibility::{ForgetTicket, Visibility, VisibilityChange};
mod progression;
pub use progression::{
    ProgressionCursor, ProgressionPackets, ProgressionProjectionError, ProgressionProjector,
    RankEffectPackets, project_rank_effect,
};
mod session_output;
pub use session_output::{
    BatchLimits, EventSequencer, LoginPossession, LoginProjection, LoginProjectionLimits,
    ReplicationMessage, SessionBatch, SessionProjectionError,
};

mod enchantments;
pub use enchantments::project_enchantments;

mod appraisal;

pub use progression::SkillPackets;

mod ui;
pub use ui::project_ui;

mod crafting;

mod portal;
pub use portal::{PortalBatch, PortalPhase, PortalView, project_portal};

mod server_motion;
pub use server_motion::project_server_motion;

mod staff;
pub use staff::{
    project_staff_broadcast, project_staff_heal, project_staff_response, project_staff_teleport,
    project_staff_text,
};

mod group_actions;
mod group_profile;

mod social_actions;

pub mod item_experience;

pub mod experience;

pub mod staff_magic;

mod inventory;
pub use inventory::InventoryProjection;

mod training_notice;
pub use training_notice::project_training_notice;

mod object_views;
pub use object_views::{ObjectProjection, physics_sequences, project_accepted_server_motion};

mod crafting_proficiency;
pub use crafting_proficiency::{
    CraftingProficiencyProjection, CraftingSkillSpend, project_crafting_commit,
};

pub mod cast_output;
pub mod combat_actions;

pub mod attribute_transfer;
pub mod skill_devices;

mod enchantment_expiry;

mod locomotion;

mod target_query;
