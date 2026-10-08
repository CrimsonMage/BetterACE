//! Immutable avatar asset closure decoded only by fingerprint-admitted DAT workers.
mod magic_damage;
mod missile;
mod presence;
mod restore;
mod stats;
pub use magic_damage::prepare_player_magic_damage;
pub use restore::{PlayerColdAssets, PlayerColdPolicy, prepare_loaded_avatar};
mod inventory;
pub use inventory::{PreparedPlayerInventory, prepare_player_inventory};
mod enchantments;
pub use enchantments::{player_enchantment_definition, prepare_player_physical_qualities};
pub use stats::{PlayerStatProjection, prepare_player_stats};
use std::{collections::BTreeMap, sync::Arc};
pub struct PreparedAvatarDat {
    pub character: Arc<crate::character_assets::PreparedCharacterAssets>,
    pub quality_filter: Arc<bace_dat::QualityFilter>,
    pub vitals: bace_dat::VitalTable,
    pub spells: Arc<bace_dat::SpellTable>,
    pub shape: Arc<bace_physics::CollisionShape>,
    pub locomotion: Arc<bace_motion::AnimatedLocomotion>,
    pub motions: bace_dat::MotionTable,
    pub animations: BTreeMap<u32, bace_dat::Animation>,
    pub maneuvers: Vec<bace_gameplay_api::weapon_combat::PhysicalManeuver>,
}
mod equipment;
pub use equipment::{
    EquipmentPhysicalInput, EquipmentPhysicalViewInput, prepare_equipment_physical,
    prepare_equipment_physical_view,
};

mod activation;
mod wield;
pub use activation::prepare_item_activation_requirements;

pub use wield::{prepare_wield_policy, prepare_wield_slot};
