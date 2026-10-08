//! Official ACE QualityFilter and GDLE ACQualityFilter 0x0E010001.
use crate::{
    DatArchive, DatError, DatTableLimits, DatTableVersion,
    table_reader::{TableReader, load_record},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityFilter {
    pub ints: Vec<u32>,
    pub int64s: Vec<u32>,
    pub bools: Vec<u32>,
    pub floats: Vec<u32>,
    pub data_ids: Vec<u32>,
    pub instance_ids: Vec<u32>,
    pub strings: Vec<u32>,
    pub positions: Vec<u32>,
    pub attributes: Vec<u32>,
    pub secondary_attributes: Vec<u32>,
    pub skills: Vec<u32>,
}
impl QualityFilter {
    pub const RECORD_ID: u32 = 0x0e010001;
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
        let mut budget = limits.max_entries;
        let first = group::<8>(&mut reader, &mut budget)?;
        let second = group::<3>(&mut reader, &mut budget)?;
        reader.finish()?;
        let [
            ints,
            int64s,
            bools,
            floats,
            data_ids,
            instance_ids,
            strings,
            positions,
        ] = first;
        let [attributes, secondary_attributes, skills] = second;
        Ok(Self {
            ints,
            int64s,
            bools,
            floats,
            data_ids,
            instance_ids,
            strings,
            positions,
            attributes,
            secondary_attributes,
            skills,
        })
    }
    /// GDLE ACQualityFilter::GetNumIntStats/GetNumFloatStats both cap at 512.
    pub fn allows_int(&self, key: u32) -> bool {
        key < 512 && self.ints.contains(&key)
    }
    pub fn allows_float(&self, key: u32) -> bool {
        key < 512 && self.floats.contains(&key)
    }
}
fn group<const N: usize>(
    reader: &mut TableReader<'_>,
    budget: &mut usize,
) -> Result<[Vec<u32>; N], DatError> {
    let mut counts = [0usize; N];
    for count in &mut counts {
        *count = reader.u32()? as usize;
    }
    let count = counts
        .iter()
        .try_fold(0usize, |n, c| n.checked_add(*c))
        .ok_or(DatError::Format("quality filter count overflow"))?;
    reader.count(count, 4)?;
    *budget = budget
        .checked_sub(count)
        .ok_or(DatError::Format("quality filter aggregate count limit"))?;
    let mut result = std::array::from_fn(|_| Vec::new());
    for (values, count) in result.iter_mut().zip(counts) {
        values.reserve(count);
        for _ in 0..count {
            values.push(reader.u32()?);
        }
    }
    Ok(result)
}
