//! Bounded immutable persistence projection, never a second mutable player owner.
use bace_gameplay_api::CharacterBinding;
use bace_types::EntityId;
#[derive(Debug)]
pub struct PlayerReadSnapshot {
    pub(crate) target_selection: Option<bace_gameplay_api::selection::TargetSelection>,
    pub(crate) gag: Option<crate::GagRecovery>,
    pub(crate) recall_destinations: crate::RecallDestinationSnapshot,
    pub(crate) equipment_mana: Option<crate::EquipmentManaRecovery>,
    pub(crate) skill_values: Vec<bace_character::SkillValues>,
    pub(crate) combat_mode: Option<u32>,
    pub(crate) staff: Option<bace_gameplay_api::staff::StaffRegistration>,
    pub(crate) entry_friends: Vec<bace_gameplay_api::social::SocialFriend>,
    pub(crate) entry_motion: Option<bace_motion::SourceMotionState>,
    pub(crate) entry_physics: bace_physics::AcceptedState,
    pub(crate) chat_age: Option<u64>,
    pub(crate) physical_recovery: f64,
    pub(crate) operation: Option<(PlayerSnapshotOperation, u64)>,
    pub(crate) binding: CharacterBinding,
    pub(crate) tick: u64,
    pub(crate) character: crate::CharacterReadSnapshot,
    pub(crate) world: crate::PlayerWorldSnapshot,
    pub(crate) death: Option<crate::PlayerDeathState>,
    pub(crate) social: Option<bace_social::SocialPreferences>,
    pub(crate) portal_links: Option<bace_interactions::PortalLinks>,
    pub(crate) recovery: Option<bace_magic::CastRecovery>,
    pub(crate) enchantments: Option<bace_magic::EnchantmentRegistry>,
    pub(crate) items: Vec<bace_inventory::InventoryItem>,
    pub(crate) item_experience: Vec<crate::PreparedItemExperience>,
    pub(crate) item_enchantments: Vec<(EntityId, bace_magic::EnchantmentRegistry)>,
}
impl PlayerReadSnapshot {
    pub fn target_selection(&self) -> Option<bace_gameplay_api::selection::TargetSelection> {
        self.target_selection
    }
    pub fn gag(&self) -> Option<crate::GagRecovery> {
        self.gag
    }
    pub fn recall_destinations(&self) -> &crate::RecallDestinationSnapshot {
        &self.recall_destinations
    }
    pub fn equipment_mana(&self) -> Option<&crate::EquipmentManaRecovery> {
        self.equipment_mana.as_ref()
    }
    pub fn item_experience(&self) -> &[crate::PreparedItemExperience] {
        &self.item_experience
    }
    pub fn skill_values(&self) -> &[bace_character::SkillValues] {
        &self.skill_values
    }
    pub fn combat_mode(&self) -> Option<u32> {
        self.combat_mode
    }
    pub fn staff(&self) -> Option<bace_gameplay_api::staff::StaffRegistration> {
        self.staff
    }
    /// Immutable accepted body frame; contains no client-requested transform.
    pub fn entry_friends(&self) -> &[bace_gameplay_api::social::SocialFriend] {
        &self.entry_friends
    }
    pub fn entry_motion(&self) -> Option<bace_motion::SourceMotionState> {
        self.entry_motion
    }
    pub fn entry_physics(&self) -> bace_physics::AcceptedState {
        self.entry_physics
    }
    pub fn chat_age(&self) -> Option<u64> {
        self.chat_age
    }
    pub fn items(&self) -> &[bace_inventory::InventoryItem] {
        &self.items
    }

    pub fn physical_recovery(&self) -> f64 {
        self.physical_recovery
    }
    pub fn operation(&self) -> Option<(PlayerSnapshotOperation, u64)> {
        self.operation
    }
    pub fn binding(&self) -> CharacterBinding {
        self.binding
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn character(&self) -> &crate::CharacterReadSnapshot {
        &self.character
    }
    pub fn world(&self) -> crate::PlayerWorldSnapshot {
        self.world
    }
    pub fn death(&self) -> Option<&crate::PlayerDeathState> {
        self.death.as_ref()
    }
    pub fn social(&self) -> Option<&bace_social::SocialPreferences> {
        self.social.as_ref()
    }
    pub fn portal_links(&self) -> Option<&bace_interactions::PortalLinks> {
        self.portal_links.as_ref()
    }
    pub fn recovery(&self) -> Option<bace_magic::CastRecovery> {
        self.recovery
    }
    pub fn enchantments(&self) -> Option<&bace_magic::EnchantmentRegistry> {
        self.enchantments.as_ref()
    }
    pub fn item_enchantments(&self) -> &[(EntityId, bace_magic::EnchantmentRegistry)] {
        &self.item_enchantments
    }
}

#[derive(Clone, Debug)]
pub struct PlayerSnapshotOutcome {
    pub correlation: u64,
    pub result: Result<std::sync::Arc<PlayerReadSnapshot>, crate::CharacterRegistrationError>,
}
#[derive(Clone, Copy, Debug)]
pub struct PlayerSnapshotRequest {
    pub correlation: u64,
    pub binding: CharacterBinding,
    pub operation: Option<(PlayerSnapshotOperation, u64)>,
}

/// Namespace and exact pending owner identity; unrelated reservations cannot
/// authorize a valuable-operation baseline capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerSnapshotOperation {
    Portal(u64),
    Inventory(u64),
    Pet(u64),
    PhysicalAmmo(u64),
    Npc { ticket: u64 },
    NpcHandIn { operation: u64 },
    Allegiance(u64),
    StaffGag(u64),
    PlayerDeath(u64),
    PveDeath(u64),
    StaffSpell(u64),
    Skill(u64),
    AttributeTransfer(u64),
    Crafting(u64),
}
