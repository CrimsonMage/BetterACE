//! Immutable candidate physical state for an inventory equipment transaction.
//! This is not an admission command: slot changes remain gated until the joint
//! registry/item-XP/vital/attachment transition has been validated and frozen.
use bace_types::EntityId;
use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct PreparedEquipmentPhysical {
    pub actor: EntityId,
    pub before_revision: u64,
    pub source: Arc<bace_combat::preparation::PreparedPhysicalRefreshSource>,
    pub motions: Vec<(u32, f32, Arc<bace_motion::PreparedMotionChain>)>,
    pub locomotion_styles: Vec<Arc<bace_motion::AnimatedLocomotion>>,
    pub death_motions: Vec<bace_motion::PreparedDeathMotion>,
    pub magic_damage: bace_magic::MagicDamageProfile,
    pub server_magic: crate::PreparedMagicAssetBatch,
    pub skills: crate::PreparedCharacterSkillInputs,
    pub vital_inputs: crate::PreparedVitalInputs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentVitalChange {
    pub before: [u32; 3],
    pub after: [u32; 3],
    pub maxima: [u32; 3],
    pub after_revision: u64,
    pub gear_health: Option<u32>,
}
pub struct PreparedEquipmentRequest {
    pub request: crate::InventoryPreparedRequest,
    pub wield: bace_inventory::WieldPolicy,
    pub effects: crate::PreparedEquipmentEffectInputs,
    pub slots: Vec<bace_inventory::WieldSlotItem>,
    /// Authored accepted-style transition, retained with the exact equipment hold.
    pub stance: Option<Arc<bace_motion::PreparedMotionChain>>,
}
