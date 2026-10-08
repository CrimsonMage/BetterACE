//! Source material, clothing palette, gem-count, quality and burden mutations.
use crate::{
    TreasureError, TreasureRandom, ace_tables,
    death_treasure::MutationContext,
    treasure_properties::*,
    treasure_random::{inclusive, unit},
    treasure_tables::*,
};
use bace_content::WeenieV1;
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn material(&mut self, w: &WeenieV1) -> Result<i32, TreasureError> {
        if let Some(data) = int(w, "TsysMutationData")? {
            let key = ((data & 255) as u32, self.profile.tier.clamp(1, 6) as u32);
            if let Some(rows) = self.assets.material_base.get(&key) {
                let draw = unit(self.random)?;
                let mut sum = 0.0f32;
                for row in rows {
                    sum += row.probability;
                    if draw < f64::from(sum) {
                        if row.material == en("MaterialType", "Ivory")? as u32 {
                            return Ok(row.material as i32);
                        }
                        if let Some(rows) = self.assets.material_group.get(&(row.material, key.1)) {
                            let draw = unit(self.random)?;
                            let mut sum = 0.0f32;
                            for row in rows {
                                sum += row.probability;
                                if draw < f64::from(sum) {
                                    return Ok(row.material as i32);
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
        self.default_material(w)
    }
    fn default_material(&mut self, w: &WeenieV1) -> Result<i32, TreasureError> {
        let entry = inclusive(self.random, 0, 4)? as usize;
        let item = int(w, "ItemType")?.unwrap_or(0);
        let kind = w.weenie_type as i32;
        let group = if kind == en("WeenieType", "Caster")? {
            Some(3)
        } else if kind == en("WeenieType", "Clothing")? {
            if item == en("ItemType", "Armor")? {
                Some(0)
            } else if item == en("ItemType", "Clothing")? {
                Some(5)
            } else {
                None
            }
        } else if kind == en("WeenieType", "MissileLauncher")?
            || kind == en("WeenieType", "Missile")?
        {
            Some(1)
        } else if kind == en("WeenieType", "MeleeWeapon")? {
            Some(2)
        } else if kind == en("WeenieType", "Generic")? {
            if item == en("ItemType", "Jewelry")? {
                Some(3)
            } else if item == en("ItemType", "MissileWeapon")? {
                Some(4)
            } else {
                None
            }
        } else {
            None
        };
        const NAMES: [[&str; 5]; 6] = [
            ["Copper", "Bronze", "Iron", "Steel", "Silver"],
            ["Oak", "Teak", "Mahogany", "Pine", "Ebony"],
            ["Brass", "Ivory", "Gold", "Steel", "Diamond"],
            ["RedGarnet", "Jet", "BlackOpal", "FireOpal", "Emerald"],
            ["Granite", "Ceramic", "Porcelain", "Alabaster", "Marble"],
            ["Linen", "Wool", "Velvet", "Satin", "Silk"],
        ];
        group.map_or(Ok(0), |g| en("MaterialType", NAMES[g][entry]))
    }
    pub(crate) fn color(&mut self, w: &mut WeenieV1) -> Result<(), TreasureError> {
        let (Some(material), Some(data), Some(clothing)) = (
            int(w, "MaterialType")?,
            int(w, "TsysMutationData")?,
            did(w, "ClothingBase")?,
        ) else {
            return Ok(());
        };
        if material <= 0 {
            return Ok(());
        }
        let code = ((data >> 16) & 255) as u32;
        let stored = self.assets.material_colors.get(&(material as u32, code));
        if stored.is_none() && code != 0 {
            return Ok(());
        }
        let palettes = self
            .assets
            .clothing_palettes
            .get(&clothing)
            .ok_or(TreasureError::MissingAsset("ClothingTable", clothing))?;
        let fallback: [crate::TreasureColorRow; 18] =
            std::array::from_fn(|i| crate::TreasureColorRow {
                palette: i as u32 + 1,
                probability: 1.0,
            });
        let rows = stored.map(Vec::as_slice).unwrap_or(&fallback);
        let total = rows
            .iter()
            .filter(|v| palettes.contains_key(&v.palette))
            .map(|v| f64::from(v.probability))
            .sum::<f64>() as f32;
        if total == 0.0 {
            return Ok(());
        }
        let draw = unit(self.random)? * f64::from(total);
        let mut sum = 0.0f32;
        for row in rows {
            let Some(&icon) = palettes.get(&row.palette) else {
                continue;
            };
            sum += row.probability;
            if draw < f64::from(sum) {
                if row.palette > 0 && icon > 0 {
                    sd(w, "Icon", icon)?;
                    si(w, "PaletteTemplate", row.palette as i32)?;
                    sf(w, "Shade", unit(self.random)?)?;
                }
                break;
            }
        }
        Ok(())
    }
    pub(crate) fn gems(&mut self, w: &mut WeenieV1) -> Result<(), TreasureError> {
        self.gems_with_max(w, 5)
    }
    pub(crate) fn gems_with_max(
        &mut self,
        w: &mut WeenieV1,
        max: i32,
    ) -> Result<(), TreasureError> {
        let count = if let Some(data) = int(w, "TsysMutationData")? {
            let code = ((data >> 8) & 255) as u8;
            if let Some(rows) = self
                .assets
                .gem_counts
                .get(&(code, self.profile.tier.min(6)))
            {
                let draw = unit(self.random)?;
                let mut sum = 0.0f32;
                let mut result = 0;
                for &(count, chance) in rows {
                    sum += chance;
                    if chance > 0.0 {
                        result = count
                    }
                    if draw < f64::from(sum) {
                        result = count;
                        break;
                    }
                }
                result
            } else {
                0
            }
        } else {
            inclusive(self.random, 1, max)?
        };
        si(w, "GemCount", count)?;
        let material = gem(self.profile.tier, self.random)?.1;
        si(w, "GemType", material)
    }
    pub(crate) fn roll_workmanship(&mut self, w: &mut WeenieV1) -> Result<(), TreasureError> {
        let v = indexed(
            "WorkmanshipChance",
            "workmanshipChances",
            (self.profile.tier.clamp(1, 6) - 1) as usize,
            0.0,
            self.random,
        )?;
        si(w, "ItemWorkmanship", v)
    }
    pub(crate) fn quality(&mut self, interval: bool) -> Result<f32, TreasureError> {
        let tier = self.profile.tier;
        let chances = ace_tables::float_sequence("QualityChance", "QualityChancePerTier")
            .ok_or_else(|| missing("QualityChance", "QualityChancePerTier"))?;
        if (unit(self.random)? - f64::from(self.profile.loot_quality_mod)).max(0.0)
            >= f64::from(chances[(tier - 1) as usize])
        {
            return Ok(0.0);
        }
        let field = format!("T{tier}_QualityChances");
        let chances = ace_tables::float_sequence("QualityChance", &field)
            .ok_or_else(|| missing("QualityChance", &field))?;
        let rng = unit(self.random)?;
        for (i, &cur) in chances.iter().enumerate() {
            if rng < f64::from(cur) && cur >= self.profile.loot_quality_mod {
                if !interval {
                    return Ok((i + 1) as f32);
                }
                let prev = if i > 0 { chances[i - 1] } else { 0.0 };
                let dx = cur - prev;
                let dy = 1.0f32 / chances.len() as f32;
                return Ok(
                    (f64::from(dy) * ((rng - f64::from(prev)) / f64::from(dx) + i as f64)) as f32,
                );
            }
        }
        Ok(0.0)
    }
    pub(crate) fn burden(&mut self, w: &mut WeenieV1, weapon: bool) -> Result<(), TreasureError> {
        let Some(old) = int(w, "EncumbranceVal")? else {
            return Ok(());
        };
        let quality = self.quality(true)?;
        if quality == 0.0 {
            return Ok(());
        }
        let bulk =
            if weapon { 0.5f32 } else { 0.25f32 } * float(w, "BulkMod")?.unwrap_or(1.0) as f32;
        let modifier = 1.0 - quality * (1.0 - bulk);
        let v = number(f64::from(old as f32 * modifier).round_ties_even())?.max(1);
        si(w, "EncumbranceVal", v)
    }
    pub(crate) fn wield_level(&mut self, w: &mut WeenieV1) -> Result<(), TreasureError> {
        if self.profile.tier < 7 {
            return Ok(());
        }
        let level = if self.profile.tier == 8 && unit(self.random)? < f64::from(0.9f32) {
            180
        } else {
            150
        };
        si(w, "WieldRequirements", en("WieldRequirement", "Level")?)?;
        si(w, "WieldDifficulty", level)?;
        si(w, "WieldSkillType", 1)
    }
}
#[cfg(test)]
#[path = "treasure_material_tests.rs"]
mod tests;
