//! Pinned ACE ContractTable/Contract/Position layouts at 47edade3; AGPL-3.0-only.
use crate::{
    DatError, DatTableLimits, ModelFrame,
    env_cell::{fixed_count, frame},
    table_reader::TableReader,
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
pub struct ContractLocation {
    pub cell: u32,
    pub frame: ModelFrame,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Contract {
    pub version: u32,
    pub id: u32,
    pub name: String,
    pub description: String,
    pub progress_description: String,
    pub start_npc: String,
    pub end_npc: String,
    pub stamped_quest: String,
    pub started_quest: String,
    pub finished_quest: String,
    pub progress_quest: String,
    pub timer_quest: String,
    pub repeat_time_quest: String,
    pub start_location: ContractLocation,
    pub end_location: ContractLocation,
    pub quest_location: ContractLocation,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ContractTable {
    pub id: u32,
    pub bucket_size: u16,
    pub contracts: BTreeMap<u32, Contract>,
}
fn string(r: &mut TableReader<'_>) -> Result<String, DatError> {
    let s = r.pstring()?;
    r.align()?;
    Ok(s)
}
fn location(r: &mut TableReader<'_>) -> Result<ContractLocation, DatError> {
    Ok(ContractLocation {
        cell: r.u32()?,
        frame: frame(r)?,
    })
}
impl ContractTable {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let id = 0x0e00001d;
        let mut r = TableReader::new(bytes, id, limits)?;
        let count = usize::from(r.u16()?);
        let bucket_size = r.u16()?;
        fixed_count(&mut r, count, 12 + 11 * 4 + 3 * 32)?;
        let mut contracts = BTreeMap::new();
        for _ in 0..count {
            let key = r.u32()?;
            let value = Contract {
                version: r.u32()?,
                id: r.u32()?,
                name: string(&mut r)?,
                description: string(&mut r)?,
                progress_description: string(&mut r)?,
                start_npc: string(&mut r)?,
                end_npc: string(&mut r)?,
                stamped_quest: string(&mut r)?,
                started_quest: string(&mut r)?,
                finished_quest: string(&mut r)?,
                progress_quest: string(&mut r)?,
                timer_quest: string(&mut r)?,
                repeat_time_quest: string(&mut r)?,
                start_location: location(&mut r)?,
                end_location: location(&mut r)?,
                quest_location: location(&mut r)?,
            };
            if contracts.insert(key, value).is_some() {
                return Err(DatError::Format("duplicate contract key"));
            }
        }
        r.finish()?;
        Ok(Self {
            id,
            bucket_size,
            contracts,
        })
    }
}
