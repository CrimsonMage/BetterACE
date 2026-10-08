//! ACE.DatLoader SecondaryAttributeTable / Attribute2ndBase at the pinned ACE commit.
use crate::{DatError, SkillFormula, table_reader::TableReader};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VitalTable {
    pub health: SkillFormula,
    pub stamina: SkillFormula,
    pub mana: SkillFormula,
}
impl VitalTable {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        let mut r = TableReader::new(bytes, 0x0e000003, Default::default())?;
        let mut formula = || {
            Ok::<_, DatError>(SkillFormula {
                w: r.u32()?,
                x: r.u32()?,
                y: r.u32()?,
                z: r.u32()?,
                attribute1: r.u32()?,
                attribute2: r.u32()?,
            })
        };
        let table = Self {
            health: formula()?,
            stamina: formula()?,
            mana: formula()?,
        };
        r.finish()?;
        Ok(table)
    }
}
