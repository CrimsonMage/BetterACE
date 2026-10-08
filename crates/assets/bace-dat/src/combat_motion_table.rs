//! Official ACE CombatManeuverTable/CombatManeuver layouts at 47edade3.
//! Copyright ACE contributors; AGPL-3.0-only. Authored order is significant.
use crate::{DatError, DatTableLimits, table_reader::TableReader};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CombatManeuver {
    pub style: u32,
    pub attack_height: u32,
    pub attack_type: u32,
    pub min_skill_level: u32,
    pub motion: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatManeuverTable {
    pub id: u32,
    pub maneuvers: Vec<CombatManeuver>,
}
impl CombatManeuverTable {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let id = u32::from_le_bytes(
            bytes
                .get(..4)
                .ok_or(DatError::Format("truncated CMT"))?
                .try_into()
                .map_err(|_| DatError::Format("CMT ID"))?,
        );
        if id >> 24 != 0x30 {
            return Err(DatError::Format("wrong CMT record ID"));
        }
        let mut reader = TableReader::new(bytes, id, limits)?;
        let count = reader.u32()? as usize;
        reader.count(count, 20)?;
        reader.reserve_entries(count)?;
        let mut maneuvers = Vec::with_capacity(count);
        for _ in 0..count {
            maneuvers.push(CombatManeuver {
                style: reader.u32()?,
                attack_height: reader.u32()?,
                attack_type: reader.u32()?,
                min_skill_level: reader.u32()?,
                motion: reader.u32()?,
            });
        }
        reader.finish()?;
        Ok(Self { id, maneuvers })
    }
    pub fn motions(
        &self,
        style: u32,
        height: u32,
        attack_type: u32,
    ) -> impl Iterator<Item = &CombatManeuver> {
        self.maneuvers.iter().filter(move |m| {
            m.style == style && m.attack_height == height && m.attack_type == attack_type
        })
    }
}
