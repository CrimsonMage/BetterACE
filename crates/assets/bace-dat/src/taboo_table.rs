//! Pinned ACE TabooTable / TabooTableEntry layout. AGPL-3.0-only.
//! Entries retain authored order: ACE searches only the first category.
use crate::{
    DatArchive, DatError, DatTableLimits, DatTableVersion,
    table_reader::{TableReader, load_record},
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabooEntry {
    pub flags: u32,
    pub unknown1: u32,
    pub unknown2: u16,
    pub patterns: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabooTable {
    pub marker: u8,
    pub entries: Vec<TabooEntry>,
}
impl TabooTable {
    pub const RECORD_ID: u32 = 0x0e00001e;
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn load_verified(
        archive: &mut DatArchive,
        version: DatTableVersion,
    ) -> Result<Self, DatError> {
        Self::decode(&load_record(
            archive,
            Self::RECORD_ID,
            version,
            DatTableLimits::default(),
        )?)
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let mut reader = TableReader::new(bytes, Self::RECORD_ID, limits)?;
        let marker = reader.u8()?;
        let count = usize::from(reader.u8()?);
        reader.count(count, 14)?;
        reader.reserve_entries(count)?;
        let mut entries = Vec::with_capacity(count);
        let mut flags = BTreeSet::new();
        for _ in 0..count {
            let key = reader.u32()?;
            if !flags.insert(key) {
                return Err(DatError::Format("duplicate taboo category"));
            }
            let unknown1 = reader.u32()?;
            let unknown2 = reader.u16()?;
            let n = reader.u32()? as usize;
            reader.count(n, 1)?;
            reader.reserve_entries(n)?;
            let mut patterns = Vec::with_capacity(n);
            for _ in 0..n {
                patterns.push(reader.dotnet_string()?);
            }
            entries.push(TabooEntry {
                flags: key,
                unknown1,
                unknown2,
                patterns,
            });
        }
        reader.finish()?;
        Ok(Self { marker, entries })
    }
}
