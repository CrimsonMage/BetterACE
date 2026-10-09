use crate::{DddError, DddLimits};
use bace_dat::DatArchive;
use bace_wire::DddDatabase;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordMetadata {
    pub database: DddDatabase,
    pub object_id: u32,
    pub resource_type: u32,
    pub iteration: u32,
    pub raw_size: u32,
    /// Includes the four-byte original-length prefix when compressed.
    pub transfer_size: u32,
    pub compressed: bool,
}
#[derive(Clone, Debug)]
pub struct DatabaseMetadata {
    pub database: DddDatabase,
    pub iteration: u32,
    pub records: Vec<RecordMetadata>,
}
/// Immutable metadata prepared on bounded asset workers, never on simulation.
/// The caller must validate source DAT fingerprints before publishing this catalog.
pub struct DddCatalog {
    pub(crate) databases: Vec<DatabaseMetadata>,
    pub(crate) iteration_indexes: Vec<BTreeMap<u32, Vec<usize>>>,
    pub(crate) transfer_ready: bool,
}
impl DddCatalog {
    /// Read only the ACE iteration records from fingerprint-admitted archives.
    /// This catalog is sufficient to finish an up-to-date login or reject an
    /// outdated client while patching is disabled. It MUST NOT be used with an
    /// enabled `DddSession`, because it has no transferable record metadata.
    pub fn from_archive_indexes(
        archives: &mut [(DddDatabase, DatArchive)],
        limits: DddLimits,
    ) -> Result<Self, DddError> {
        let mut databases = Vec::with_capacity(archives.len());
        for (database, archive) in archives {
            validate_dataset(*database, archive)?;
            let iteration = archive_iteration(archive, limits)?;
            databases.push(DatabaseMetadata {
                database: *database,
                iteration,
                records: Vec::new(),
            });
        }
        let mut catalog = Self::new(databases, limits)?;
        catalog.transfer_ready = false;
        Ok(catalog)
    }

    /// Build exact transfer metadata on a blocking asset-preparation worker.
    /// Every record is read and compressed once so Begin's byte count matches
    /// the later on-demand preparation. The caller owns fingerprint admission.
    pub fn from_archives(
        archives: &mut [(DddDatabase, DatArchive)],
        limits: DddLimits,
    ) -> Result<Self, DddError> {
        let mut databases = Vec::with_capacity(archives.len());
        for (database, archive) in archives {
            validate_dataset(*database, archive)?;
            let iteration = archive_iteration(archive, limits)?;
            if archive.records().len() > limits.max_catalog_records {
                return Err(DddError::Capacity);
            }
            let ids: Vec<u32> = archive.records().keys().copied().collect();
            let mut records = Vec::with_capacity(ids.len());
            for id in ids {
                let record = *archive.records().get(&id).ok_or(DddError::InvalidCatalog)?;
                // The version control record is metadata, not DDD file data.
                if record.id == ITERATION_RECORD_ID || record.iteration == 0 {
                    continue;
                }
                let resource_type =
                    resource_type(*database, record.id).ok_or(DddError::UnsupportedRecordType)?;
                if *database == DddDatabase::Cell {
                    if record.size as usize > limits.max_record_bytes {
                        return Err(DddError::Capacity);
                    }
                    records.push(RecordMetadata {
                        database: *database,
                        object_id: record.id,
                        resource_type,
                        iteration: record.iteration,
                        raw_size: record.size,
                        transfer_size: record.size,
                        compressed: false,
                    });
                } else {
                    let prepared = crate::prepare_archive_record(
                        archive,
                        *database,
                        record.id,
                        resource_type,
                        limits,
                    )?;
                    records.push(prepared.metadata());
                }
            }
            databases.push(DatabaseMetadata {
                database: *database,
                iteration,
                records,
            });
        }
        Self::new(databases, limits)
    }

    pub fn new(mut databases: Vec<DatabaseMetadata>, limits: DddLimits) -> Result<Self, DddError> {
        limits.validate()?;
        if databases.len() > 4 {
            return Err(DddError::InvalidCatalog);
        }
        databases.sort_by_key(|d| database_order(d.database));
        let mut total = 0usize;
        let mut previous = None;
        for database in &mut databases {
            if previous == Some(database.database)
                || database.iteration == 0
                || database.iteration > limits.max_iterations
            {
                return Err(DddError::InvalidCatalog);
            }
            previous = Some(database.database);
            total = total
                .checked_add(database.records.len())
                .ok_or(DddError::Capacity)?;
            if total > limits.max_catalog_records {
                return Err(DddError::Capacity);
            }
            database.records.sort_by_key(|record| record.object_id);
            let mut prior_id = None;
            for record in &database.records {
                if record.database != database.database
                    || prior_id == Some(record.object_id)
                    || record.iteration > database.iteration
                    || record.iteration == 0
                    || record.raw_size as usize > limits.max_record_bytes
                    || record.transfer_size as usize > limits.max_record_bytes
                    || (record.compressed
                        && (record.transfer_size < 4 || record.transfer_size >= record.raw_size))
                    || (!record.compressed && record.transfer_size != record.raw_size)
                {
                    return Err(DddError::InvalidCatalog);
                }
                prior_id = Some(record.object_id);
            }
        }
        if !databases.iter().any(|d| d.database == DddDatabase::Portal)
            || !databases
                .iter()
                .any(|d| d.database == DddDatabase::Language)
        {
            return Err(DddError::MissingDatabase);
        }
        let iteration_indexes = databases
            .iter()
            .map(|database| {
                let mut index: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
                for (offset, record) in database.records.iter().enumerate() {
                    index.entry(record.iteration).or_default().push(offset);
                }
                index
            })
            .collect();
        Ok(Self {
            databases,
            iteration_indexes,
            transfer_ready: true,
        })
    }
    pub fn record(&self, database: DddDatabase, id: u32) -> Option<RecordMetadata> {
        let records = &self
            .databases
            .iter()
            .find(|d| d.database == database)?
            .records;
        let index = records
            .binary_search_by_key(&id, |record| record.object_id)
            .ok()?;
        Some(records[index])
    }

    pub fn database_iteration(&self, database: DddDatabase) -> Option<u32> {
        self.databases
            .iter()
            .find(|entry| entry.database == database)
            .map(|entry| entry.iteration)
    }
}

const ITERATION_RECORD_ID: u32 = 0xFFFF_0001;

fn validate_dataset(database: DddDatabase, archive: &DatArchive) -> Result<(), DddError> {
    let expected = match database {
        DddDatabase::Portal | DddDatabase::HighRes => 1,
        DddDatabase::Cell => 2,
        DddDatabase::Language => 3,
    };
    if archive.header().dataset != expected {
        return Err(DddError::InvalidCatalog);
    }
    Ok(())
}

fn archive_iteration(archive: &mut DatArchive, limits: DddLimits) -> Result<u32, DddError> {
    let raw = archive.read(ITERATION_RECORD_ID)?;
    let count = raw
        .get(..4)
        .ok_or(DddError::InvalidIterations)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four bytes")))?;
    if count == 0 || count > limits.max_iterations {
        return Err(DddError::InvalidIterations);
    }
    // ACE Iteration.Unpack stores negative run lengths followed by positive
    // starting iterations. Validate the complete bounded record, not only its
    // advertised count, before using it to negotiate a client version.
    if (raw.len() - 4) % 8 != 0 {
        return Err(DddError::InvalidIterations);
    }
    let mut remaining = count;
    let mut seen = vec![false; count as usize + 1];
    for pair in raw[4..].chunks_exact(8) {
        let length = i32::from_le_bytes(pair[..4].try_into().expect("four bytes"));
        let start = i32::from_le_bytes(pair[4..].try_into().expect("four bytes"));
        if length >= 0 || start <= 0 {
            return Err(DddError::InvalidIterations);
        }
        let length = length.unsigned_abs();
        let end = (start as u32)
            .checked_add(length - 1)
            .ok_or(DddError::InvalidIterations)?;
        if length > remaining || end > count {
            return Err(DddError::InvalidIterations);
        }
        for iteration in start as u32..=end {
            if std::mem::replace(&mut seen[iteration as usize], true) {
                return Err(DddError::InvalidIterations);
            }
        }
        remaining -= length;
    }
    if remaining != 0 {
        return Err(DddError::InvalidIterations);
    }
    Ok(count)
}

/// Pinned ACE DatFile.GetFileType mapping. Unknown types fail closed when
/// patching is enabled; silently sending a guessed type corrupts client DATs.
fn resource_type(database: DddDatabase, id: u32) -> Option<u32> {
    match database {
        DddDatabase::Cell => Some(match id & 0xffff {
            0xffff => 1,
            0xfffe => 2,
            _ => 3,
        }),
        DddDatabase::Portal => {
            let by_prefix = match id >> 24 {
                0x01 => 6,
                0x02 => 7,
                0x03 => 8,
                0x04 => 10,
                0x05 => 11,
                0x06 => 12,
                0x08 => 13,
                0x09 => 14,
                0x0a => 15,
                0x0d => 16,
                0x0f => 24,
                0x10 => 25,
                0x11 => 26,
                0x12 => 27,
                0x13 => 28,
                0x14 => 29,
                0x15 => 30,
                0x16 => 31,
                0x17 => 32,
                0x18 => 33,
                0x20 => 34,
                0x22 => 36,
                0x25 => 38,
                0x26 => 39,
                0x27 => 40,
                0x30 => 0x1000_000d,
                0x31 => 41,
                0x32 => 42,
                0x33 => 43,
                0x34 => 44,
                0x39 => 45,
                0x40 => 46,
                0x78 => 49,
                _ => 0,
            };
            if by_prefix != 0 {
                return Some(by_prefix);
            }
            match id >> 16 {
                0x0e01 => return Some(0x1000_000c),
                0x0e02 => return Some(23),
                _ => {}
            }
            Some(match id {
                0x0e00_0002 => 0x1000_0002,
                0x0e00_0003 => 0x1000_0003,
                0x0e00_0004 => 0x1000_0004,
                0x0e00_0007 => 17,
                0x0e00_000d => 18,
                0x0e00_000e => 0x1000_0005,
                0x0e00_000f => 0x1000_0006,
                0x0e00_0018 => 0x1000_0009,
                0x0e00_001a => 19,
                0x0e00_001d => 0x1000_0010,
                0x0e00_001e => 20,
                0x0e00_001f => 21,
                0x0e00_0020 => 22,
                _ => return None,
            })
        }
        DddDatabase::Language => Some(match id >> 24 {
            0x21 => 35,
            0x23 => 37,
            0x41 => 48,
            _ => return None,
        }),
        DddDatabase::HighRes => (id >> 24 == 0x06).then_some(12),
    }
}
pub(crate) fn database_order(database: DddDatabase) -> u8 {
    match database {
        DddDatabase::Portal => 0,
        DddDatabase::Language => 1,
        DddDatabase::Cell => 2,
        DddDatabase::HighRes => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires user-supplied DATs; set BACE_DAT_DIRECTORY and run --ignored"]
    fn supplied_dat_records_have_ace_resource_types() {
        let root = std::path::PathBuf::from(std::env::var("BACE_DAT_DIRECTORY").unwrap());
        for (database, name) in [
            (DddDatabase::Portal, "client_portal.dat"),
            (DddDatabase::Language, "client_local_English.dat"),
            (DddDatabase::Cell, "client_cell_1.dat"),
        ] {
            let archive = DatArchive::open(root.join(name)).unwrap();
            let maximum = archive
                .records()
                .values()
                .map(|r| r.size)
                .max()
                .unwrap_or(0);
            let total: u64 = archive.records().values().map(|r| u64::from(r.size)).sum();
            eprintln!(
                "{name}: {} records, largest {} bytes, raw sum {} bytes",
                archive.records().len(),
                maximum,
                total
            );
            let unknown: Vec<_> = archive
                .records()
                .values()
                .filter(|record| record.id != ITERATION_RECORD_ID && record.iteration != 0)
                .filter(|record| resource_type(database, record.id).is_none())
                .map(|record| record.id)
                .take(8)
                .collect();
            assert!(
                unknown.is_empty(),
                "{name}: unknown record types: {unknown:08x?}"
            );
        }
    }
}
