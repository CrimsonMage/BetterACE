//! Complete pinned world extraction; MariaDB executes unchanged input SQL.
use crate::{MariaDbStaging, SqlStagingError, StagingManifest};
use bace_content::{WeenieTemplate, WorldRecordV1};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug)]
pub struct CompleteStagedWorld {
    pub manifest: StagingManifest,
    pub weenies: Vec<WeenieTemplate>,
    /// Ordered by frozen table inventory, then the original primary key.
    pub records: Vec<WorldRecordV1>,
}

/// Complete schema support does not claim implemented runtime gameplay.
/// The caller verifies the release archive fingerprint before extraction.
pub fn import_complete_world(
    backend: &mut MariaDbStaging,
    dump: &Path,
) -> Result<CompleteStagedWorld, SqlStagingError> {
    if !dump.is_file() {
        return Err(SqlStagingError::Input);
    }
    backend.with_loaded(dump, |db, digest| {
        let mut world = crate::sql_extract::extract_mode(db, digest, true)?;
        let records = crate::world_extract::extract(db)?;
        let mut counts = BTreeMap::<&str, u64>::new();
        for record in &records {
            *counts.entry(record.table_name()).or_default() += 1;
        }
        for spec in crate::world_extract::WORLD_TABLES {
            if world
                .manifest
                .table_row_counts
                .get(spec.name)
                .copied()
                .unwrap_or(0)
                != counts.get(spec.name).copied().unwrap_or(0)
            {
                return Err(SqlStagingError::Extraction(format!(
                    "source/extracted {} row counts differ",
                    spec.name
                )));
            }
        }
        world.manifest.source_release = "operator-supplied-complete-world-sql".into();
        Ok(CompleteStagedWorld {
            manifest: world.manifest,
            weenies: world.weenies,
            records,
        })
    })
}
