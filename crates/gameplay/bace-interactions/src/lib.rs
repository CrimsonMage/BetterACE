//! Doors, locks, switches, portals, lifestones and books.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod door;
pub use door::{DoorAction, DoorAuthority, DoorError, DoorMotion, DoorPhysics, DoorReset};

mod portals;
pub mod world_policies;
pub use world_policies::{
    RegionPreload, WorldPolicies, WorldPolicyError, adjacent_landblocks, apartment_landblocks,
};
pub mod recalls;
pub use portals::{
    PortalAccess, PortalAnchor, PortalError, PortalKind, PortalLinkMutation, PortalLinks,
    PortalPosition,
};
pub use recalls::{
    RecallAccess, RecallError, RecallKind, RecallPolicy, check_recall, recall_delay_ticks,
    recall_moved_too_far,
};

pub mod bindings;
pub use bindings::{BindingEffect, BindingKind, check_binding, complete_binding};
pub mod player_death;
pub use player_death::{
    PlayerDeathKind, PlayerDeathPolicy, classify_player_death, death_coin_count, death_destination,
    death_item_count, keep_death_enchantment, player_corpse_decay_seconds, restored_death_vital,
};

pub mod death_items;
pub use death_items::{
    DeathDrop, DeathItemError, DeathItemPlan, DeathPossession, death_item_category,
    select_death_items,
};

mod portal_template;
pub use portal_template::PortalTemplate;
