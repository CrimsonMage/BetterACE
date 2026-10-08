//! Original material/workmanship/spell monetary value and description formulas.
use crate::{
    TreasureError, TreasureRandom, TreasureRoll, ace_tables, death_treasure::MutationContext,
    treasure_properties::*, treasure_random::unit, treasure_tables::*,
};
use bace_content::WeenieV1;
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn value(
        &mut self,
        w: &mut WeenieV1,
        _roll: &TreasureRoll,
    ) -> Result<(), TreasureError> {
        if int(w, "Value")?.is_none() {
            si(w, "Value", 0)?
        }
        let material = int(w, "MaterialType")?;
        let material_mod = ace_tables::lookup("MaterialTable", "ValueMod")
            .ok_or_else(|| missing("MaterialTable", "ValueMod"))?
            .iter()
            .find(|&&(m, _)| Some(m as i32) == material)
            .map_or(1.0, |&(_, v)| v);
        if w.weenie_type as i32 == en("WeenieType", "Gem")? {
            let old = int(w, "Value")?.unwrap_or(0);
            si(
                w,
                "Value",
                number(f64::from(old as f32 * material_mod * workmanship(w, 0)?))?,
            )?;
        } else {
            if int(w, "ArmorLevel")?.unwrap_or(0) > 0 {
                self.value_armor(w)?;
            }
            let rng = (f64::from(0.7f32) + unit(self.random)? * f64::from(1.25f32 - 0.7f32)) as f32;
            let gem = int(w, "GemType")?;
            let mut gem_value = 0.0f32;
            for i in 0..6 {
                let (c, f) = reference("GemMaterialChance", "gemMaterialChances", i)?;
                if ace_tables::gem(c, f)
                    .ok_or_else(|| missing(c, f))?
                    .iter()
                    .any(|&(_, m, _)| Some(m as i32) == gem)
                {
                    gem_value = [10.0, 50.0, 100.0, 250.0, 500.0, 1000.0][i];
                    break;
                }
            }
            let tier = [25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2000.0, 3000.0]
                [(self.profile.tier - 1) as usize];
            let old = int(w, "Value")?.unwrap_or(0);
            let mut value = old as f32 * (1.0f32 / 3.0) + material_mod * tier + gem_value;
            value *= workmanship(w, 0)? * rng;
            value += old as f32 * (1.0f32 - 1.0f32 / 3.0);
            let value = number(f64::from(value).ceil())?;
            if value > old {
                si(w, "Value", value)?;
            }
        }
        let mut sum = 0i32;
        if let Some(id) = did(w, "Spell")? {
            sum = sum
                .checked_add(self.spell(id)?.level as i32)
                .ok_or(TreasureError::Bounds)?
        }
        for p in &w.properties.spell_book {
            sum = sum
                .checked_add(self.spell(p.id as u32)?.level as i32)
                .ok_or(TreasureError::Bounds)?
        }
        let value = int(w, "Value")?
            .unwrap_or(0)
            .checked_add(
                int(w, "ItemMaxMana")?
                    .unwrap_or(0)
                    .checked_mul(2)
                    .ok_or(TreasureError::Bounds)?,
            )
            .and_then(|v| v.checked_add(sum.checked_mul(10)?))
            .ok_or(TreasureError::Bounds)?;
        si(w, "Value", value)
    }
    pub(crate) fn long_desc(&self, w: &mut WeenieV1) -> Result<(), TreasureError> {
        let name = string(w, "Name")?.unwrap_or_default();
        let descriptors = ace_tables::descriptors("CasterSlotSpells", "descriptors")
            .ok_or_else(|| missing("CasterSlotSpells", "descriptors"))?;
        let spell = did(w, "Spell")?;
        for id in spell
            .into_iter()
            .chain(w.properties.spell_book.iter().map(|p| p.id as u32))
        {
            if let Some(levels) = ace_tables::spell_levels(i64::from(id))
                && let Some(&first) = levels.first()
                && let Some((_, desc)) = descriptors.iter().find(|(s, _)| *s == first)
            {
                return ss(w, "LongDesc", format!("{name} of {desc}"));
            }
        }
        ss(w, "LongDesc", name)
    }
}
#[cfg(test)]
#[path = "treasure_value_tests.rs"]
mod tests;
