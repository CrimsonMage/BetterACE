//! Immutable authoritative inventory projection contracts.
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemPlace {
    Contained {
        container: EntityId,
        slot: u32,
        equipped: u32,
    },
    World,
    Removed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryItem {
    /// PropertyInt.Structure (92), independent of stack quantity.
    pub structure: Option<u32>,
    pub id: EntityId,
    pub revision: u64,
    pub template: u32,
    pub stack_key: u64,
    pub place: ItemPlace,
    pub stack: u32,
    pub maximum_stack: u32,
    pub unit_burden: u32,
    pub unit_value: u32,
    pub pack_slot: bool,
    pub is_container: bool,
    pub attuned: bool,
    pub trade_reserved: bool,
    pub active_pet: bool,
    pub unique: bool,
    pub quest_allowed: bool,
    pub valid_wield: u32,
    pub incompatible_wield: u32,
    pub wield_requirements_met: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryContainer {
    pub id: EntityId,
    pub revision: u64,
    /// Character whose carried/equipped inventory this root represents.
    /// House storage, hooks, chests and corpses use None even if privately owned.
    pub root_owner: Option<EntityId>,
    pub slots: u32,
    pub pack_slots: u32,
    pub burden_limit: u64,
    pub accessible: bool,
    pub open: bool,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct InventoryAuthority {
    pub actor: EntityId,
    pub busy: bool,
    pub in_range: bool,
    pub clear_path: bool,
    pub geometry_ready: bool,
    pub drop_validated: bool,
    pub source_view: Option<u64>,
    pub destination_view: Option<u64>,
    pub new_item: Option<EntityId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemChange {
    pub before: Option<InventoryItem>,
    pub after: InventoryItem,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryProposal {
    pub changes: Vec<ItemChange>,
    pub participants: Vec<(EntityId, u64)>,
    pub actor_burden: u64,
    pub requires_pickup_motion: bool,
}
/// Immutable snapshot view of one bounded transaction scope, not duplicate world state.
pub struct InventoryView<'a> {
    pub items: &'a [InventoryItem],
    pub containers: &'a [InventoryContainer],
}
