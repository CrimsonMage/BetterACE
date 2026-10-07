//! ACE.DatLoader/FileTypes/XpTable.cs at
//! 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only.
use crate::{
    DatArchive, DatError, DatTableLimits, DatTableVersion,
    table_reader::{TableReader, load_record},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XpTable {
    pub attribute_xp: Vec<u32>,
    pub vital_xp: Vec<u32>,
    pub trained_skill_xp: Vec<u32>,
    pub specialized_skill_xp: Vec<u32>,
    pub character_level_xp: Vec<u64>,
    pub character_level_skill_credits: Vec<u32>,
}
impl XpTable {
    pub const RECORD_ID: u32 = 0x0e000018;
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
        let mut counts = [0usize; 5];
        for (index, count) in counts.iter_mut().enumerate() {
            let value = reader.u32()?;
            if index < 4 && value > i32::MAX as u32 {
                return Err(DatError::Format("negative DAT XP count"));
            }
            *count = (value as usize)
                .checked_add(1)
                .ok_or(DatError::Format("DAT XP count overflow"))?;
            if *count > limits.max_entries {
                return Err(DatError::Format("DAT XP count limit"));
            }
        }
        let required = counts[..4]
            .iter()
            .try_fold(0usize, |total, count| {
                total.checked_add(count.checked_mul(4)?)
            })
            .and_then(|total| total.checked_add(counts[4].checked_mul(12)?))
            .ok_or(DatError::Format("DAT XP size overflow"))?;
        if required != reader.remaining() {
            return Err(DatError::Format("DAT XP table size mismatch"));
        }
        let mut words = |count| {
            (0..count)
                .map(|_| reader.u32())
                .collect::<Result<Vec<_>, _>>()
        };
        let attribute_xp = words(counts[0])?;
        let vital_xp = words(counts[1])?;
        let trained_skill_xp = words(counts[2])?;
        let specialized_skill_xp = words(counts[3])?;
        let character_level_xp = (0..counts[4])
            .map(|_| reader.u64())
            .collect::<Result<Vec<_>, _>>()?;
        let character_level_skill_credits = (0..counts[4])
            .map(|_| reader.u32())
            .collect::<Result<Vec<_>, _>>()?;
        reader.finish()?;
        Ok(Self {
            attribute_xp,
            vital_xp,
            trained_skill_xp,
            specialized_skill_xp,
            character_level_xp,
            character_level_skill_credits,
        })
    }
}
