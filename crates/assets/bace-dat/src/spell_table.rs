//! Pinned ACE SpellTable/SpellBase/SpellSet/SpellSetTiers. AGPL-3.0-only.
use crate::{
    DatArchive, DatError, DatTableLimits, DatTableVersion,
    spell_strings::obfuscated,
    table_reader::{TableReader, load_record},
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
pub struct SpellBase {
    pub name: String,
    pub description: String,
    pub school: u32,
    pub icon: u32,
    pub category: u32,
    pub flags: u32,
    pub base_mana: u32,
    pub range_constant: f32,
    pub range_modifier: f32,
    pub power: u32,
    pub economy_modifier: f32,
    pub formula_version: u32,
    pub component_loss: f32,
    pub meta_type: u32,
    pub meta_id: u32,
    pub enchantment: Option<(f64, f32, f32)>,
    pub portal_lifetime: Option<f64>,
    pub formula: Vec<u32>,
    pub caster_effect: u32,
    pub target_effect: u32,
    pub fizzle_effect: u32,
    pub recovery_interval: f64,
    pub recovery_amount: f32,
    pub display_order: u32,
    pub non_component_target_type: u32,
    pub mana_modifier: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SpellTable {
    pub spells: BTreeMap<u32, SpellBase>,
    pub sets: BTreeMap<u32, BTreeMap<u32, Vec<u32>>>,
}
impl SpellTable {
    pub const RECORD_ID: u32 = 0x0e00000e;
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
        let count = packed_count(&mut r, 132)?;
        let mut spells = BTreeMap::new();
        for _ in 0..count {
            let id = r.u32()?;
            let value = SpellBase::read(&mut r)?;
            if spells.insert(id, value).is_some() {
                return Err(DatError::Format("duplicate spell ID"));
            }
        }
        let count = packed_count(&mut r, 8)?;
        let mut sets = BTreeMap::new();
        for _ in 0..count {
            let id = r.u32()?;
            let count = packed_count(&mut r, 8)?;
            let mut tiers = BTreeMap::new();
            for _ in 0..count {
                let tier = r.u32()?;
                let count = r.u32()? as usize;
                r.count(count, 4)?;
                r.reserve_entries(count)?;
                let mut ids = Vec::with_capacity(count);
                for _ in 0..count {
                    ids.push(r.u32()?);
                }
                if tiers.insert(tier, ids).is_some() {
                    return Err(DatError::Format("duplicate spell-set tier"));
                }
            }
            if sets.insert(id, tiers).is_some() {
                return Err(DatError::Format("duplicate spell-set ID"));
            }
        }
        r.finish()?;
        Ok(Self { spells, sets })
    }
    /// Sparse predecessor lookup avoids ACE's unbounded expansion to HighestTier.
    pub fn set_at_level(&self, set: u32, level: u32) -> Option<&[u32]> {
        self.sets
            .get(&set)?
            .range(..=level)
            .next_back()
            .map(|(_, spells)| spells.as_slice())
    }
}
fn packed_count(r: &mut TableReader<'_>, minimum: usize) -> Result<usize, DatError> {
    let count = usize::from(r.u16()?);
    r.u16()?;
    r.count(count, minimum)?;
    r.reserve_entries(count)?;
    Ok(count)
}
impl SpellBase {
    fn read(r: &mut TableReader<'_>) -> Result<Self, DatError> {
        let (name, name_hash) = obfuscated(r)?;
        let (description, description_hash) = obfuscated(r)?;
        let school = r.u32()?;
        let icon = r.u32()?;
        let category = r.u32()?;
        let flags = r.u32()?;
        let base_mana = r.u32()?;
        let range_constant = r.f32()?;
        let range_modifier = r.f32()?;
        let power = r.u32()?;
        let economy_modifier = r.f32()?;
        let formula_version = r.u32()?;
        let component_loss = r.f32()?;
        let meta_type = r.u32()?;
        let meta_id = r.u32()?;
        if meta_type > 15 {
            return Err(DatError::Format("unsupported spell meta layout"));
        }
        let enchantment = if meta_type == 1 || meta_type == 12 {
            Some((r.f64()?, r.f32()?, r.f32()?))
        } else {
            None
        };
        let portal_lifetime = if meta_type == 7 { Some(r.f64()?) } else { None };
        let key = (name_hash % 0x12107680).wrapping_add(description_hash % 0xbeadcf45);
        let mut formula = Vec::with_capacity(8);
        for _ in 0..8 {
            let raw = r.u32()?;
            if raw > 0 {
                let component = raw.wrapping_sub(key);
                formula.push(if component > 198 {
                    component & 0xff
                } else {
                    component
                });
            }
        }
        Ok(Self {
            name,
            description,
            school,
            icon,
            category,
            flags,
            base_mana,
            range_constant,
            range_modifier,
            power,
            economy_modifier,
            formula_version,
            component_loss,
            meta_type,
            meta_id,
            enchantment,
            portal_lifetime,
            formula,
            caster_effect: r.u32()?,
            target_effect: r.u32()?,
            fizzle_effect: r.u32()?,
            recovery_interval: r.f64()?,
            recovery_amount: r.f32()?,
            display_order: r.u32()?,
            non_component_target_type: r.u32()?,
            mana_modifier: r.u32()?,
        })
    }
}
