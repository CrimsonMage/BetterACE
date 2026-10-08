//! Pinned ACE SpellComponentsTable/SpellComponentBase; no spell rules or I/O in decode.
use crate::{
    DatArchive, DatError, DatTableLimits, DatTableVersion,
    spell_strings::obfuscated,
    table_reader::{TableReader, load_record},
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
pub struct SpellComponent {
    pub name: String,
    pub category: u32,
    pub icon: u32,
    pub kind: u32,
    pub gesture: u32,
    pub time: f32,
    pub text: String,
    pub modifier: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SpellComponents {
    pub components: BTreeMap<u32, SpellComponent>,
}
impl SpellComponents {
    pub const RECORD_ID: u32 = 0x0e00000f;
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
        let mut r = TableReader::new(bytes, Self::RECORD_ID, limits)?;
        let count = usize::from(r.u16()?);
        r.align()?;
        r.count(count, 36)?;
        r.reserve_entries(count)?;
        let mut components = BTreeMap::new();
        for _ in 0..count {
            let id = r.u32()?;
            let value = SpellComponent {
                name: obfuscated(&mut r)?.0,
                category: r.u32()?,
                icon: r.u32()?,
                kind: r.u32()?,
                gesture: r.u32()?,
                time: r.f32()?,
                text: obfuscated(&mut r)?.0,
                modifier: r.f32()?,
            };
            if components.insert(id, value).is_some() {
                return Err(DatError::Format("duplicate spell component ID"));
            }
        }
        r.finish()?;
        Ok(Self { components })
    }
}
