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

#[cfg(test)]
mod sql_process_tests;
