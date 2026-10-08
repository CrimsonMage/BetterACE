//! Armor/clothing mutation pipeline from pinned LootGenerationFactory_Clothing.
use crate::{
    TreasureError, TreasureRandom, TreasureRoll, ace_tables,
    death_treasure::MutationContext,
    treasure_properties::*,
    treasure_random::{inclusive, unit},
    treasure_tables::*,
};
use bace_content::WeenieV1;
pub(crate) fn shield(w: &WeenieV1) -> Result<bool, TreasureError> {
    Ok(int(w, "CombatUse")? == Some(en("CombatUse", "Shield")?))
}
fn coverage_mask(names: &[&str]) -> Result<i32, TreasureError> {
    names
        .iter()
        .try_fold(0, |v, n| Ok(v | en("CoverageMask", n)?))
}
fn outerwear() -> Result<i32, TreasureError> {
    coverage_mask(&[
        "OuterwearChest",
        "OuterwearAbdomen",
        "OuterwearUpperArms",
        "OuterwearLowerArms",
        "OuterwearUpperLegs",
        "OuterwearLowerLegs",
        "Head",
        "Hands",
        "Feet",
    ])
}
fn extremities() -> Result<i32, TreasureError> {
    coverage_mask(&["Head", "Hands", "Feet"])
}
fn armor_script(w: &WeenieV1, roll: &TreasureRoll) -> Result<&'static str, TreasureError> {
    let is_shield = shield(w)?;
    let script = match (roll.armor_type, is_shield) {
        (11, true) => "covenant_shield",
        (11, false) => "covenant_armor",
        (19, true) => "olthoi_shield",
        (19, false) => "olthoi_armor",
        (_, true) => "shield_level",
        _ => {
            let coverage = int(w, "ClothingPriority")?.unwrap_or(0);
            if coverage & extremities()? != 0 {
                "armor_level_extremity"
            } else if coverage & outerwear()? != 0 {
                "armor_level_non_extremity"
            } else {
                return Err(TreasureError::InvalidTemplate(w.weenie_id));
            }
        }
    };
    Ok(script)
}
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn mutate_armor(
        &mut self,
        w: &mut WeenieV1,
        magical: bool,
        roll: &mut TreasureRoll,
    ) -> Result<(), TreasureError> {
        let material = self.material(w)?;
        if material > 0 {
            si(w, "MaterialType", material)?;
        }
        self.color(w)?;
        self.gems_with_max(w, 6)?;
        self.roll_workmanship(w)?;
        if has_filter(w, "EncumbranceVal")? {
            self.burden(w, false)?;
        }
        if self.profile.tier > 6 && int(w, "ArmorLevel")?.unwrap_or(0) <= 0 {
            self.wield_level(w)?;
        }
        if int(w, "ArmorLevel")?.unwrap_or(0) > 0 {
            let script = armor_script(w, roll)?;
            let props = ["WieldRequirements", "WieldSkillType", "WieldDifficulty"];
            let old = props
                .map(|n| int(w, n))
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            self.script(w, &format!("ArmorLevel.{script}.txt"))?;
            if (25..=28).contains(&roll.armor_type) {
                for (n, v) in props.into_iter().zip(old) {
                    if let Some(v) = v {
                        si(w, n, v)?
                    } else {
                        ri(w, n)?
                    }
                }
            }
        }
        if has_filter(w, "ArmorModVsType")? {
            for n in [
                "ArmorModVsFire",
                "ArmorModVsCold",
                "ArmorModVsAcid",
                "ArmorModVsElectric",
            ] {
                self.armor_resistance(w, n)?;
            }
        }
        if magical {
            self.assign_magic(w, roll)?;
        } else {
            self.clear_magic(w, false)?;
        }
        if self.profile.tier > 6 && !(25..=28).contains(&roll.armor_type) {
            if let Some(set) = self.equipment_set(w)? {
                si(w, "EquipmentSetId", set)?;
            } else {
                ri(w, "EquipmentSetId")?;
            }
        }
        if self.profile.tier == 8 {
            self.gear_rating(w, roll)?;
        }
        self.value(w, roll)?;
        self.long_desc(w)
    }
    fn equipment_set(&mut self, w: &WeenieV1) -> Result<Option<i32>, TreasureError> {
        if self.profile.tier < 6
            || int(w, "ArmorLevel")?.unwrap_or(0) <= 0
            || int(w, "ClothingPriority")?.unwrap_or(0) & outerwear()? == 0
        {
            return Ok(None);
        }
        if crate::treasure_tables::roll(
            "EquipmentSetChance",
            "armorSetChance",
            self.profile.loot_quality_mod,
            self.random,
        )? == 0
        {
            return Ok(None);
        }
        Ok(Some(sequence(
            "EquipmentSetChance",
            "armorSets",
            self.random,
        )?))
    }
    fn armor_resistance(&mut self, w: &mut WeenieV1, name: &str) -> Result<(), TreasureError> {
        let Some(old) = float(w, name)? else {
            return Ok(());
        };
        if self.profile.tier < 2 {
            return Ok(());
        }
        let chances = ace_tables::float_sequence("ArmorModVsTypeChance", "TierChances")
            .ok_or_else(|| missing("ArmorModVsTypeChance", "TierChances"))?;
        if unit(self.random)? >= f64::from(chances[(self.profile.tier - 1) as usize]) {
            return Ok(());
        }
        let level = indexed(
            "ArmorModVsTypeChance",
            "qualityLevels",
            (self.profile.tier - 1) as usize,
            self.profile.loot_quality_mod,
            self.random,
        )?;
        let rng = unit(self.random)? * f64::from(0.15f32 - (-0.05f32)) + f64::from(-0.05f32);
        sf(
            w,
            name,
            (old + f64::from(level as f32 * 0.15f32) + rng).clamp(-2.0, 2.0),
        )
    }
    pub(crate) fn value_armor(&mut self, w: &mut WeenieV1) -> Result<(), TreasureError> {
        let bulk = float(w, "BulkMod")?.unwrap_or(1.0);
        let size = float(w, "SizeMod")?.unwrap_or(1.0);
        let min = bulk.min(size) as f32;
        let max = bulk.max(size) as f32;
        let rng = unit(self.random)? * f64::from(max - min) + f64::from(min);
        let al = int(w, "ArmorLevel")?.unwrap_or(0);
        let squared = al.checked_mul(al).ok_or(TreasureError::Bounds)?;
        let addon = number(f64::from(squared as f32 / 10.0) * rng)?;
        let value = int(w, "Value")?
            .unwrap_or(0)
            .checked_add(addon)
            .ok_or(TreasureError::Bounds)?;
        si(w, "Value", value)
    }
    pub(crate) fn gear_rating(
        &mut self,
        w: &mut WeenieV1,
        treasure: &TreasureRoll,
    ) -> Result<(), TreasureError> {
        if self.profile.tier != 8 || shield(w)? {
            return Ok(());
        }
        if roll(
            "GearRatingChance",
            "RatingChance",
            self.profile.loot_quality_mod,
            self.random,
        )? == 0
        {
            return Ok(());
        }
        let armor = int(w, "ArmorLevel")?.unwrap_or(0) > 0;
        let field = if armor {
            "ArmorRating"
        } else if matches!(treasure.item_type, 3 | 7 | 25) {
            "ClothingJewelryRating"
        } else {
            return Ok(());
        };
        let value = roll(
            "GearRatingChance",
            field,
            self.profile.loot_quality_mod,
            self.random,
        )?;
        if value == 0 {
            return Ok(());
        }
        let rng = inclusive(self.random, 0, 1)?;
        let property = if armor {
            ["GearCritDamage", "GearCritDamageResist"][rng as usize]
        } else if matches!(treasure.item_type, 7 | 25) {
            ["GearDamage", "GearDamageResist"][rng as usize]
        } else {
            ["GearHealingBoost", "GearMaxHealth"][rng as usize]
        };
        si(w, property, value)?;
        if treasure.armor_type != 25 {
            self.set_wield_level(w, 180)?;
        }
        Ok(())
    }
    fn set_wield_level(&self, w: &mut WeenieV1, level: i32) -> Result<(), TreasureError> {
        let requirement = int(w, "WieldRequirements")?.unwrap_or(0);
        let level_type = en("WieldRequirement", "Level")?;
        if requirement == en("WieldRequirement", "Invalid")? {
            si(w, "WieldRequirements", level_type)?;
            si(w, "WieldSkillType", en("Skill", "Axe")?)?;
            si(w, "WieldDifficulty", level)?;
        } else if requirement == level_type {
            if int(w, "WieldDifficulty")?.unwrap_or(0) < level {
                si(w, "WieldDifficulty", level)?;
            }
        } else {
            si(w, "WieldRequirements2", level_type)?;
            si(w, "WieldSkillType2", en("Skill", "Axe")?)?;
            si(w, "WieldDifficulty2", level)?;
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "treasure_armor_tests.rs"]
mod tests;
