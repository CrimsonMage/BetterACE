//! Treasure tables and generated items.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod create_list;
pub use create_list::{CreateEntry, LootError, select_create_list};
mod rares;
pub use rares::{RareError, RareEvaluator};
mod graph;
pub use graph::{GraphError, LootDrop, LootGraph, LootScratch};
mod materialize;
mod mutation;
pub use materialize::materialize_drop;
mod treasure_random;
pub use treasure_random::{TreasureError, TreasureRandom};
mod wielded_treasure;
pub use wielded_treasure::{WieldedTreasure, is_stackable, set_treasure_stack};
mod death_treasure_types;
pub use death_treasure_types::{
    TreasureAssets, TreasureColorRow, TreasureMaterialRow, TreasureRoll, TreasureSpell,
};

pub mod ace_tables;
mod treasure_template_closure;
pub use treasure_template_closure::pinned_treasure_templates;

mod treasure_selection;
mod treasure_tables;
pub use treasure_selection::{TreasureCategory, select_treasure};
mod treasure_mutations;
pub use treasure_mutations::{MutationScript, MutationScripts};
mod create_list_materialize;
pub use create_list_materialize::materialize_create_list;
mod death_treasure;
pub use death_treasure::DeathTreasure;
mod creature_equipment;
mod creature_equipment_sort;
mod treasure_armor;
mod treasure_cantrips;
mod treasure_items;
mod treasure_magic;
mod treasure_materials;
mod treasure_properties;
mod treasure_scrolls;
mod treasure_special;
mod treasure_spells;
mod treasure_value;
mod treasure_weapons;
pub use creature_equipment::{
    PreparedContainerItem, PreparedCreatureEquipment, append_creature_inventory,
    generate_creature_equipment, materialize_container_tree, materialize_create_list_tree,
};
mod create_list_selection;
pub use create_list_selection::generate_create_list_selection;
mod player_olthoi;
pub use player_olthoi::{PlayerSlag, roll_player_gland, roll_player_slag};
mod player_no_corpse;
pub use player_no_corpse::generate_player_no_corpse_create_list;
mod vendor_shop;
pub use vendor_shop::{PreparedVendorShopTree, materialize_vendor_shop_create_list};
