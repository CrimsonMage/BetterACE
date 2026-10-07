use crate::{DddError, DddLimits};
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
}
impl DddCatalog {
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
}
pub(crate) fn database_order(database: DddDatabase) -> u8 {
    match database {
        DddDatabase::Portal => 0,
        DddDatabase::Language => 1,
        DddDatabase::Cell => 2,
        DddDatabase::HighRes => 3,
    }
}
