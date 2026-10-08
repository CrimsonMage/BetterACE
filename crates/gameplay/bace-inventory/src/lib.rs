//! Containers, equipment, stacks and ownership.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod transfer;
pub use transfer::{ContainerState, ItemState, TransferError, TransferProposal, propose_transfer};
mod actions;
mod model;
pub use actions::{propose_grant, propose_inventory, propose_take};
pub use model::{
    InventoryAuthority, InventoryContainer, InventoryItem, InventoryProposal, InventoryView,
    ItemChange, ItemPlace,
};

pub use actions::propose_take_template;

mod components;
pub use components::propose_take_items;
pub use components::{has_required_components, propose_component_use};

pub use actions::propose_item_changes;

mod pet_device;
pub use pet_device::{propose_pet_charge, propose_pet_release};

mod split;
pub use split::{StackSplitPreparation, propose_stack_split};
mod physical;
pub use physical::{InventoryCylinder, inventory_pickup_motion, inventory_use_distance};
mod wield;
pub use wield::{WieldCriterion, WieldFailure, WieldPolicy, WieldValues, check_wield_requirements};
mod activation;
pub use activation::{
    ActivationFailure, ActivationObject, ActivationRequirements, ActivationValues,
    check_item_activation,
};

mod activation_messages;
pub use activation_messages::ActivationMessage;

mod wield_slots;
pub use actions::propose_equipment_inventory;
pub use wield_slots::{
    WieldSlotError, WieldSlotItem, check_weapon_collision, check_wield_slots, wield_slot_available,
};

mod npc_grants;
pub use npc_grants::{propose_npc_grants, propose_npc_inspection};
mod vendor_purchase;
pub use vendor_purchase::{propose_vendor_purchase, select_vendor_currency_debits};
