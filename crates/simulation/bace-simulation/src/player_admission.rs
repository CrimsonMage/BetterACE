//! Cold prepared inputs transfer together to the one live simulation owner.
use bace_types::EntityId;
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedItemSpellTarget {
    pub item: EntityId,
    pub item_type: u32,
    pub resist_magic: u32,
    pub non_projectile_immune: bool,
}
pub struct PreparedPlayerAdmission {
    pub chat_eligibility: bace_social::ChatEligibility,
    pub binding: bace_gameplay_api::CharacterBinding,
    pub actor: bace_entity::Actor,
    pub locomotion: Arc<bace_motion::AnimatedLocomotion>,
    pub locomotion_styles: Vec<Arc<bace_motion::AnimatedLocomotion>>,
    pub death_motions: Vec<bace_motion::PreparedDeathMotion>,
    pub state: crate::OwnedPlayerState,
    pub properties: bace_entity::EntityProperties,
    pub combatant: bace_entity::Combatant,
    pub caster: crate::MagicCaster,
    pub magic_damage: bace_magic::MagicDamageProfile,
    pub server_magic: crate::PreparedMagicAssetBatch,
    pub physical: Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
    pub physical_motions: Vec<(u32, f32, Arc<bace_motion::PreparedMotionChain>)>,
    pub physical_source: Arc<bace_combat::preparation::PreparedPhysicalRefreshSource>,
    pub skills: crate::PreparedCharacterSkillInputs,
    pub vital_inputs: crate::PreparedVitalInputs,
    pub items: Vec<bace_inventory::InventoryItem>,
    pub item_spell_targets: Vec<PreparedItemSpellTarget>,
    pub containers: Vec<bace_inventory::InventoryContainer>,
    pub presence: bace_social::SocialPresence,
    pub staff: bace_gameplay_api::staff::StaffRegistration,
    pub portal_access: bace_interactions::PortalAccess,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAdmissionError {
    Identity,
    Geometry,
    Capacity,
    Character,
    Inventory,
    Magic,
    Combat,
    Social,
    Staff,
    State,
}
impl PreparedPlayerAdmission {
    pub fn actor_id(&self) -> EntityId {
        self.binding.actor
    }
}

pub struct PlayerAdmissionRequest {
    pub correlation: u64,
    pub prepared: Box<PreparedPlayerAdmission>,
}
impl PlayerAdmissionRequest {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && self.prepared.physical_motions.len() <= 128
            && self.prepared.server_magic.definitions.len() <= 32
            && self.prepared.server_magic.projectile_shapes.len() <= 32
            && self.prepared.locomotion_styles.len() <= 4
            && self.prepared.death_motions.len() <= 16
            && self.prepared.physical_source.qualities.len() <= 32768
            && self.prepared.physical_source.equipment.len() <= 128
            && self.prepared.vital_inputs.equipped_health.len() <= 64
            && self.prepared.items.len() <= 1023
            && self.prepared.item_spell_targets.len() <= 1023
            && self.prepared.containers.len() <= 1024
            && self.prepared.state.item_experience.len() <= 1023
            && self.prepared.state.item_enchantments.len() <= 1023
            && self
                .prepared
                .state
                .item_enchantments
                .iter()
                .map(|(_, r)| r.entries().len())
                .sum::<usize>()
                <= 65536
    }
}
pub struct PlayerAdmissionOutcome {
    pub correlation: u64,
    pub binding: bace_gameplay_api::CharacterBinding,
    /// Rejection returns the complete preparation; it was never adopted by World.
    pub result: Result<(), (PlayerAdmissionError, Box<PreparedPlayerAdmission>)>,
}

/// Trusted host completion after the Online lease and entry output are admitted.
#[derive(Clone, Copy, Debug)]
pub struct PlayerEnteredRequest {
    pub correlation: u64,
    pub binding: bace_gameplay_api::CharacterBinding,
}
#[derive(Clone, Copy, Debug)]
pub struct PlayerEnteredOutcome {
    pub correlation: u64,
    pub binding: bace_gameplay_api::CharacterBinding,
    pub result: Result<(), bace_gameplay_api::UiError>,
}
