//! Frozen typed content DTOs and atomically replaceable immutable catalogs.
mod bounded;
mod catalog;
mod clothing;
mod model;
mod properties;
mod validation;

pub use catalog::{Catalog, CatalogSnapshot, Publication};
pub use clothing::*;
pub use model::{Property, SparseProperties, WeenieTemplate, WeenieV1};
pub use properties::*;
pub use validation::{ContentError, ContentLimits};
mod death_treasure;
pub use death_treasure::DeathTreasureV1;

mod world_record;
mod world_rows;
pub use world_record::WorldRecordV1;
pub use world_rows::*;
mod world_index;
pub use world_index::{InstanceLinkIndexV1, InstanceLinkTargetV1, LandblockIndexV1};
mod world_references;
pub use world_references::{WorldReferenceIssue, WorldReferenceReport, inspect_world_references};
mod rare_profile;
pub use rare_profile::*;
mod loot_graph;
pub use loot_graph::*;
mod loot_mutation;
pub use loot_mutation::LootMutationV1;
mod character_start;
pub use character_start::{
    CharacterStartAreaV1, CharacterStartGearV1, CharacterStartProfileV1, CharacterStartSpellV1,
};
mod creature_names;
pub use creature_names::{
    CreatureNameIndexV1, CreatureNameV1, TemplateClassIdentityV1, TemplateClassIndexV1,
};
mod animation_swap;
pub use animation_swap::{AnimationSwapEditV1, AnimationSwapOperationV1, AnimationSwapPatchV1};
mod treasure_table_set;
pub use treasure_table_set::*;
