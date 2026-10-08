//! Checked legacy conversion. JSON is restricted to this tooling boundary.
mod class_names;
mod enum_names;
mod integer_property_names;
mod json;
mod json_export;
mod legacy_names;
mod lifestoned;
mod lifestoned_complex;
mod lifestoned_helpers;
mod mariadb;
mod motion_names;
mod property_names;
mod sql_export;
mod sql_extract;
mod sql_process;
mod sql_specs;
mod staging;
mod staging_inventory;
mod strict_json;

pub use json::{ImportError, import_weenie_json};
pub use json_export::export_weenie_json;
pub use sql_export::export_weenie_sql;
pub use staging::{
    SqlStagingBackend, SqlStagingError, StagedWorld, StagingManifest, import_staged_world,
    probe_mariadb,
};

pub use mariadb::{MariaDbBinaries, MariaDbStaging};

mod emote_script;
mod emote_script_schema;
#[cfg(test)]
mod sql_process_tests;
pub use emote_script::{export_emote_script, import_emote_script};
mod loot_profile;
pub use loot_profile::{export_loot_json, export_loot_sql, import_loot_json};

mod complete_world;
mod world_extract;
pub use complete_world::{CompleteStagedWorld, import_complete_world};

mod emote_script_position;

mod clothing_export;
mod clothing_import;
mod clothing_json;
pub use clothing_export::export_clothing_json;
pub use clothing_import::{MAX_CLOTHING_BYTES, import_clothing_json};
