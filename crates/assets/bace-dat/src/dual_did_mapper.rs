//! Pinned ACE DualDidMapper.Unpack, used by Spell.GetComponentWCID. Names are
//! byte-length UTF-8 PStrings without alignment; counts are compressed UInt32.
use crate::{
    DatArchive, DatError, DatTableLimits, DatTableVersion,
    table_reader::{TableReader, load_record},
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DualDidMapper {
    pub numbering: [u8; 4],
    pub client_ids: BTreeMap<u32, u32>,
    pub client_names: BTreeMap<u32, String>,
    pub server_ids: BTreeMap<u32, u32>,
    pub server_names: BTreeMap<u32, String>,
}
impl DualDidMapper {
    pub const COMPONENT_RECORD_ID: u32 = 0x27000002;
    pub const MATERIAL_RECORD_ID: u32 = 0x27000000;
    pub fn load_components_verified(
        archive: &mut DatArchive,
        version: DatTableVersion,
    ) -> Result<Self, DatError> {
        Self::decode(&load_record(
            archive,
            Self::COMPONENT_RECORD_ID,
            version,
            DatTableLimits::default(),
        )?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        Self::decode_record(bytes, Self::COMPONENT_RECORD_ID, limits)
    }
    /// The expected record comes from accepted content, never a client packet.
    /// All DualDidMapper instances share the source layout and bounded parser.
    pub fn decode_record(
        bytes: &[u8],
        expected_id: u32,
        limits: DatTableLimits,
    ) -> Result<Self, DatError> {
        if expected_id >> 24 != 0x27 {
            return Err(DatError::Format("DualDidMapper record class"));
        }
        let mut r = TableReader::new(bytes, expected_id, limits)?;
        let mut numbering = [0; 4];
        numbering[0] = r.u8()?;
        let client_ids = ids(&mut r)?;
        numbering[1] = r.u8()?;
        let client_names = names(&mut r)?;
        numbering[2] = r.u8()?;
        let server_ids = ids(&mut r)?;
        numbering[3] = r.u8()?;
        let server_names = names(&mut r)?;
        r.finish()?;
        Ok(Self {
            numbering,
            client_ids,
            client_names,
            server_ids,
            server_names,
        })
    }
}
fn ids(r: &mut TableReader<'_>) -> Result<BTreeMap<u32, u32>, DatError> {
    let count = r.smart_count(8)?;
    let mut result = BTreeMap::new();
    for _ in 0..count {
        let key = r.u32()?;
        let value = r.u32()?;
        if result.insert(key, value).is_some() {
            return Err(DatError::Format("duplicate DualDidMapper ID"));
        }
    }
    Ok(result)
}
fn names(r: &mut TableReader<'_>) -> Result<BTreeMap<u32, String>, DatError> {
    let count = r.smart_count(5)?;
    let mut result = BTreeMap::new();
    for _ in 0..count {
        let key = r.u32()?;
        let len = usize::from(r.u8()?);
        if len > r.limits.max_string_bytes {
            return Err(DatError::Format("mapper name limit"));
        }
        let value = String::from_utf8_lossy(r.take(len)?).into_owned();
        if result.insert(key, value).is_some() {
            return Err(DatError::Format("duplicate DualDidMapper name"));
        }
    }
    Ok(result)
}
