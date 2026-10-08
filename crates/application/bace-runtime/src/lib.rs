//! Service composition, workers, startup and shutdown.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

pub mod authentication;
pub mod authentication_pool;
pub mod control;
pub mod dat_distribution;
pub mod entry;
mod exercise;
pub mod network;
pub mod persistence_thread;
mod publication;
pub mod saves;
pub mod simulation;
pub mod supervisor;
pub mod supervisor_child;
mod worker;

pub use publication::{PublicationError, PublicationResult, load_catalog, publish_pending_once};
pub mod character_assets;
pub mod character_creation;
pub mod character_preparation;
pub mod creation_profile;
pub mod game_housing;
pub mod game_inventory;
pub mod game_lifecycle;
pub mod game_login;
pub mod game_messages;
pub mod pack_io;
pub mod random_keys;
pub mod rare_saves;
pub mod readiness;
pub mod world_content;

mod content_inbox;
mod mapped_validation;
pub mod native_publication;

pub mod death_saves;

pub mod ui_saves;

pub mod enchantment_saves;

pub mod name_preparation;
pub mod progression_saves;

pub mod crafting_saves;
pub mod crafting_service;
pub mod inventory_service;

pub mod game_random;

pub mod pet_saves;
pub mod pet_service;
pub mod shard_control;
pub mod skill_saves;
pub mod skill_service;

pub mod player_saves;

pub mod crafting_output;

pub mod crafting_requests;

pub mod skill_device_output;

pub mod magic_recovery;

pub mod region_geometry;
pub mod world_admission;

pub mod magic_preparation;
pub mod native_magic_assets;

pub mod native_player;

pub mod npc_persistence;

pub mod portal_preparation;

pub mod world_saves;

pub mod physical_assets;
pub mod physical_preparation;
pub mod portal_output;

pub mod generated_saves;
pub mod generator_items;
pub mod generator_preparation;
pub mod generator_treasure;
pub mod region_activation;
pub mod treasure_assets;

pub mod generated_retirement;
pub mod generator_equipment;

pub mod generator_catalog;

pub mod generator_equipment_combat;

pub mod generated_recovery;
pub mod generator_enchantments;
pub mod generator_spell_assets;

pub mod magic_saves;

pub mod world_settings;

pub mod stack_factory;

pub mod social_saves;

pub mod placement_saves;

pub mod portal_saves;

pub mod allegiance_saves;
pub mod staff_map;
pub mod staff_map_worker;

pub mod allegiance_players;
pub mod chat_service;
pub mod player_death_preparation;
pub mod player_death_saves;
pub mod player_death_service;

pub mod allegiance_pending;
pub mod allegiance_service;

pub mod player_death_state;

pub mod staff_commands;

pub mod death_shared_saves;

pub mod item_experience;

pub mod npc_shared_saves;

pub mod item_reward_join;

pub mod region_unload_saves;

pub mod staff_spell_saves;

pub mod player_preparation;

pub mod staff_magic_assets;

pub mod corpse_expiry_saves;

pub mod player_assets;

pub mod gameplay_dispatch;
pub mod social_service;

pub mod game_bootstrap;

pub mod player_service;

pub mod staff_game_dispatch;

pub mod region_service;

pub mod reward_service;

pub mod player_entry;

pub mod online_player_saves;

pub mod game_clock;

pub mod chat_policy;

pub mod generator_service;

pub mod player_preparation_worker;

pub mod social_lookup;

pub mod game_host;
pub mod game_runtime;

pub mod creation_assets;

pub mod portal_service;

pub mod staff_native_magic;

pub mod creation_preparation_worker;
pub mod startup_assets;

pub mod npc_recovery;
pub mod npc_region;

pub mod npc_service;

pub mod npc_sources;

pub mod equipment_effects;

pub mod visibility_service;

pub mod physical_resource_service;
pub mod visibility_assets;

pub mod equipment_mana;

pub mod inventory_equipment_output;

pub mod server_magic_assets;

pub mod staff_gags;

pub mod npc_items;

pub mod npc_combat_assets;

pub mod npc_motion_assets;
