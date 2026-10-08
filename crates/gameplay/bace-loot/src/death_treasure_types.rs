//! Prepared immutable dependencies for ACE's death-treasure factory.
use bace_content::WeenieV1;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreasureMaterialRow {
    pub material: u32,
    pub probability: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreasureColorRow {
    pub palette: u32,
    pub probability: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreasureSpell {
    pub power: u32,
    pub base_mana: u32,
    pub level: u32,
    pub formula_level: u32,
}
/// Populated on bounded asset-preparation capacity. Missing entries are errors
/// where ACE requires an asset; material-table misses retain ACE's own fallback.
#[derive(Clone, Default)]
pub struct TreasureAssets {
    pub mutation_scripts: Option<Arc<crate::MutationScripts>>,
    pub gem_counts: BTreeMap<(u8, i32), Vec<(i32, f32)>>,
    pub templates: BTreeMap<u32, Arc<WeenieV1>>,
    pub material_base: BTreeMap<(u32, u32), Vec<TreasureMaterialRow>>,
    pub material_group: BTreeMap<(u32, u32), Vec<TreasureMaterialRow>>,
    pub material_colors: BTreeMap<(u32, u32), Vec<TreasureColorRow>>,
    /// DAT ClothingTable ID -> palette-template ID -> icon DID.
    /// Original DAT hash-table order, used by CalculateObjDesc palette fallback.
    pub clothing_order: BTreeMap<u32, Vec<u32>>,
    /// ClothingBase setup applicability; icon overrides require the item setup.
    pub clothing_setups: BTreeMap<u32, BTreeSet<u32>>,
    pub clothing_palettes: BTreeMap<u32, BTreeMap<u32, u32>>,
    pub spells: BTreeMap<u32, TreasureSpell>,
    /// Source scroll template resolved by spell DID after spell-level selection.
    pub scrolls_by_spell: BTreeMap<u32, u32>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TreasureRoll {
    pub item_type: i32,
    pub armor_type: i32,
    pub weapon_type: i32,
    pub wcid: u32,
    pub base_armor_level: i32,
    pub item_difficulty: f32,
}
impl TreasureRoll {
    pub fn is_melee(&self) -> bool {
        matches!(self.weapon_type,1..=11|17..=21)
    }
    pub fn is_missile(&self) -> bool {
        matches!(self.weapon_type, 12..=15)
    }
    pub fn is_caster(&self) -> bool {
        self.weapon_type == 16
    }
    pub fn is_weapon(&self) -> bool {
        self.weapon_type != 0
    }
    pub fn is_armor(&self) -> bool {
        self.armor_type != 0
    }
}

impl TreasureAssets {
    pub(crate) fn validate(&self) -> Result<(), crate::TreasureError> {
        use crate::TreasureError;
        let mut rows = 0usize;
        for values in self
            .material_base
            .values()
            .chain(self.material_group.values())
        {
            rows = rows
                .checked_add(values.len())
                .ok_or(TreasureError::Capacity)?;
            if values.iter().any(|r| {
                !r.probability.is_finite() || r.probability < 0.0 || r.material > i32::MAX as u32
            }) {
                return Err(TreasureError::Bounds);
            }
        }
        for values in self.material_colors.values() {
            rows = rows
                .checked_add(values.len())
                .ok_or(TreasureError::Capacity)?;
            if values.iter().any(|r| {
                !r.probability.is_finite() || r.probability < 0.0 || r.palette > i32::MAX as u32
            }) {
                return Err(TreasureError::Bounds);
            }
        }
        for values in self.gem_counts.values() {
            rows = rows
                .checked_add(values.len())
                .ok_or(TreasureError::Capacity)?;
            if values
                .iter()
                .any(|&(count, p)| !p.is_finite() || p <= 0.0 || count < 0)
            {
                return Err(TreasureError::Bounds);
            }
        }
        if rows > 65536
            || self.spells.len() > 65536
            || self.scrolls_by_spell.len() > 65536
            || self.clothing_palettes.len() > 65536
            || self.clothing_order.len() > 65536
            || self.clothing_setups.len() > 65536
        {
            return Err(TreasureError::Capacity);
        }
        for values in self.clothing_palettes.values() {
            rows = rows
                .checked_add(values.len())
                .ok_or(TreasureError::Capacity)?;
        }
        for values in self.clothing_order.values() {
            rows = rows
                .checked_add(values.len())
                .ok_or(TreasureError::Capacity)?;
        }
        for values in self.clothing_setups.values() {
            rows = rows
                .checked_add(values.len())
                .ok_or(TreasureError::Capacity)?;
        }
        if rows > 262144 {
            return Err(TreasureError::Capacity);
        }
        if self.spells.values().any(|s| {
            s.power > i32::MAX as u32
                || s.base_mana > i32::MAX as u32
                || s.level > i32::MAX as u32
                || s.formula_level > i32::MAX as u32
        }) {
            return Err(TreasureError::Bounds);
        }
        Ok(())
    }
}
