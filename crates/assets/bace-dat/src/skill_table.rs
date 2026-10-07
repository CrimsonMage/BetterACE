//! ACE.DatLoader/FileTypes/SkillTable.cs and Entity/{SkillBase,SkillFormula}.cs
//! at 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only.
use crate::{
    DatArchive, DatError, DatTableLimits, DatTableVersion,
    table_reader::{TableReader, load_record},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillFormula {
    pub w: u32,
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub attribute1: u32,
    pub attribute2: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SkillBase {
    pub description: String,
    pub name: String,
    pub icon_id: u32,
    pub trained_cost: i32,
    /// Includes the trained cost; it is not the incremental upgrade price.
    pub specialized_cost: i32,
    pub category: u32,
    pub chargen_use: u32,
    pub min_level: u32,
    pub formula: SkillFormula,
    pub upper_bound: f64,
    pub lower_bound: f64,
    pub learn_modifier: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SkillTable {
    /// Preserved container metadata; bucket size is not an allocation request.
    pub bucket_size: u16,
    pub skills: BTreeMap<u32, SkillBase>,
}
impl SkillTable {
    pub const RECORD_ID: u32 = 0x0e000004;
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
        let count = usize::from(reader.u16()?);
        let bucket_size = reader.u16()?;
        reader.count(count, 84)?;
        let mut skills = BTreeMap::new();
        for _ in 0..count {
            let id = reader.u32()?;
            let value = SkillBase {
                description: reader.pstring()?,
                name: reader.pstring()?,
                icon_id: reader.u32()?,
                trained_cost: reader.u32()? as i32,
                specialized_cost: reader.u32()? as i32,
                category: reader.u32()?,
                chargen_use: reader.u32()?,
                min_level: reader.u32()?,
                formula: SkillFormula {
                    w: reader.u32()?,
                    x: reader.u32()?,
                    y: reader.u32()?,
                    z: reader.u32()?,
                    attribute1: reader.u32()?,
                    attribute2: reader.u32()?,
                },
                upper_bound: reader.f64()?,
                lower_bound: reader.f64()?,
                learn_modifier: reader.f64()?,
            };
            if skills.insert(id, value).is_some() {
                return Err(DatError::Format("duplicate DAT skill ID"));
            }
        }
        reader.finish()?;
        Ok(Self {
            bucket_size,
            skills,
        })
    }
}
