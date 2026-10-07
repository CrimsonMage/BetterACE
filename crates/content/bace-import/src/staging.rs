use std::{collections::BTreeMap, path::Path, process::Command};

use bace_content::{ContentLimits, WeenieTemplate};
use thiserror::Error;

/// Extraction must account for all source tables; non-weenie world data cannot
/// silently disappear merely because a weenie-only converter is available.
#[derive(Clone, Debug)]
pub struct StagingManifest {
    pub source_sha256: String,
    pub source_release: String,
    pub upstream_pin: String,
    pub table_row_counts: BTreeMap<String, u64>,
    pub unsupported_tables: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct StagedWorld {
    pub manifest: StagingManifest,
    pub weenies: Vec<WeenieTemplate>,
}

#[derive(Debug, Error)]
pub enum SqlStagingError {
    #[error("isolated SQL staging unavailable: {0}; native runtime requires PostgreSQL only")]
    Unavailable(String),
    #[error("SQL staging/extraction failed: {0}")]
    Extraction(String),
    #[error("source contains unsupported world tables: {0:?}; no partial import published")]
    UnsupportedTables(Vec<String>),
    #[error("invalid staged weenie: {0}")]
    Content(#[from] bace_content::ContentError),
    #[error("SQL input must be an existing regular file")]
    Input,
    #[error("staging manifest is incomplete or references a different upstream baseline")]
    Manifest,
}

/// Implementations MUST create a disposable isolated MariaDB instance (or
/// container with no production connectivity), load the complete upstream dump
/// using MariaDB's SQL parser, and extract typed rows with explicit ORDER BY for
/// authored sequences. The dump recreates databases and MUST NOT be sent to an
/// existing ACE instance or PostgreSQL. Regex SQL conversion is prohibited.
///
/// `MariaDbStaging` provides a concrete weenie-only implementation. Populated
/// unsupported tables fail explicitly; production integrations must verify the
/// manifest against their chosen official release before publication.
pub trait SqlStagingBackend {
    fn extract_isolated(&mut self, dump: &Path) -> Result<StagedWorld, SqlStagingError>;
}

pub fn import_staged_world(
    backend: &mut dyn SqlStagingBackend,
    dump: &Path,
) -> Result<StagedWorld, SqlStagingError> {
    if !dump.is_file() {
        return Err(SqlStagingError::Input);
    }
    let mut world = backend.extract_isolated(dump)?;
    if world.manifest.source_sha256.len() != 64
        || !world
            .manifest
            .source_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || world.manifest.source_release.is_empty()
        || world.manifest.upstream_pin != "47edade3bd3f6044b676d4eb877c4965c7eda62b"
        || world.manifest.table_row_counts.is_empty()
    {
        return Err(SqlStagingError::Manifest);
    }
    if !world.manifest.unsupported_tables.is_empty() {
        return Err(SqlStagingError::UnsupportedTables(
            world.manifest.unsupported_tables,
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    for weenie in &mut world.weenies {
        weenie.validate(ContentLimits::default())?;
        if !ids.insert(weenie.weenie_id) {
            return Err(bace_content::ContentError::DuplicateWeenie(weenie.weenie_id).into());
        }
        weenie.canonicalize();
    }
    crate::staging_inventory::validate(&world.manifest, &world.weenies)?;
    Ok(world)
}

/// Diagnostic only. Finding a local executable does not authorize SQL execution
/// or establish that an isolated backend/extractor is configured.
pub fn probe_mariadb() -> Result<String, SqlStagingError> {
    let output = Command::new("mariadb")
        .arg("--version")
        .output()
        .map_err(|error| {
            SqlStagingError::Unavailable(format!("MariaDB client prerequisite missing: {error}"))
        })?;
    if !output.status.success() {
        return Err(SqlStagingError::Unavailable(
            "MariaDB client --version failed".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}
