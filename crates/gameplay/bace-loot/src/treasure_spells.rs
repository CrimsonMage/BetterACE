//! Pinned ACE LootGenerationFactory_Spells: item buffs and enchantments.
use crate::{
    TreasureError, TreasureRandom, TreasureRoll, ace_tables,
    death_treasure::MutationContext,
    treasure_properties as p,
    treasure_random::unit,
    treasure_tables::{en, indexed, missing},
};
use bace_content::{Property, WeenieV1};
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn assign_spells(
        &mut self,
        w: &mut WeenieV1,
        roll: &mut TreasureRoll,
    ) -> Result<(), TreasureError> {
        let mut spells = Vec::new();
        if p::int(w, "ArmorLevel")?.unwrap_or(0) > 0 || roll.is_weapon() {
            spells.extend(self.item_spells(w, roll)?);
        }
        let enchantments = self.enchantments(w, roll)?;
        let mut levels = Vec::new();
        for &id in &enchantments {
            levels.push(self.spell(id as u32)?.formula_level);
        }
        levels.sort_unstable();
        for &level in levels.iter().take(levels.len().saturating_sub(1)) {
            let rng = (0.5 + unit(self.random)?) as f32;
            roll.item_difficulty += level as f32 * 5.0 * rng;
        }
        spells.extend(enchantments);
        let cantrips = self.cantrips(w, roll)?;
        for &cantrip in &cantrips {
            let levels = spell_levels(cantrip)?;
            if levels.len() != 4 {
                return Err(TreasureError::MissingAsset(
                    "cantrip progression",
                    cantrip as u32,
                ));
            }
            let level = levels
                .iter()
                .position(|&id| id == i64::from(cantrip))
                .ok_or(TreasureError::Bounds)?;
            roll.item_difficulty += if level == 0 {
                (5.0 + unit(self.random)? * 5.0) as f32
            } else {
                (10.0 + unit(self.random)? * 10.0) as f32
            };
        }
        spells.extend(cantrips);
        if spells.len() > 64 || w.properties.spell_book.len() + spells.len() > 4096 {
            return Err(TreasureError::Capacity);
        }
        for spell in spells {
            if !w.properties.spell_book.iter().any(|p| p.id == spell) {
                w.properties.spell_book.push(Property {
                    id: spell,
                    value: 2.0,
                });
            }
        }
        Ok(())
    }
    fn item_spells(
        &mut self,
        w: &WeenieV1,
        roll: &TreasureRoll,
    ) -> Result<Vec<i32>, TreasureError> {
        let (class, field) = if p::int(w, "ArmorLevel")?.unwrap_or(0) > 0 {
            ("ArmorSpells", "armorSpells")
        } else if roll.is_melee() {
            ("MeleeSpells", "weaponMeleeSpells")
        } else if roll.is_missile() {
            ("MissileSpells", "weaponMissileSpells")
        } else if roll.is_caster() {
            ("WandSpells", "wandSpells")
        } else {
            return Ok(vec![]);
        };
        let rows = ace_tables::lookup(class, field).ok_or_else(|| missing(class, field))?;
        let mut spells = Vec::new();
        let skip_spirit = roll.is_caster() && p::int(w, "DamageType")?.unwrap_or(0) == 0;
        for &(id, chance) in rows {
            if skip_spirit && id == i64::from(en("SpellId", "SpiritDrinkerSelf1")?) {
                continue;
            }
            if interval(self.profile.loot_quality_mod, self.random)? < f64::from(chance) {
                spells.push(i32::try_from(id).map_err(|_| TreasureError::Bounds)?);
            }
        }
        self.spell_levels(&spells)
    }
    fn spell_levels(&mut self, spells: &[i32]) -> Result<Vec<i32>, TreasureError> {
        let mut result = Vec::with_capacity(spells.len());
        for &spell in spells {
            let level = indexed(
                "SpellLevelChance",
                "spellLevelChances",
                (self.profile.tier - 1) as usize,
                0.0,
                self.random,
            )?;
            let levels = spell_levels(spell)?;
            if levels.len() != 8 {
                return Err(TreasureError::MissingAsset(
                    "spell progression",
                    spell as u32,
                ));
            }
            let id = *levels
                .get((level - 1) as usize)
                .ok_or(TreasureError::Bounds)?;
            result.push(i32::try_from(id).map_err(|_| TreasureError::Bounds)?);
        }
        Ok(result)
    }
    fn enchantments(
        &mut self,
        w: &WeenieV1,
        roll: &TreasureRoll,
    ) -> Result<Vec<i32>, TreasureError> {
        let code = spell_selection_code(w, roll)?;
        if code == 0 {
            return Ok(vec![]);
        }
        let count = self.num_enchantments(roll)?;
        let mut spells = Vec::new();
        for _ in 0..count * 3 {
            if spells.len() >= count {
                break;
            }
            let spell = indexed(
                "SpellSelectionTable",
                "spellSelectionGroup",
                (code - 1) as usize,
                0.0,
                self.random,
            )?;
            if spell != 0 && !spells.contains(&spell) {
                spells.push(spell);
            }
        }
        self.spell_levels(&spells)
    }
    fn num_enchantments(&mut self, roll: &TreasureRoll) -> Result<usize, TreasureError> {
        let tier = self.profile.tier;
        if roll.is_armor() || roll.is_weapon() {
            let chances = if roll.is_caster() {
                [0.6f32, 0.6, 0.6, 0.6, 0.6, 0.75, 0.75, 0.75]
            } else {
                [0.0, 0.05, 0.1, 0.2, 0.4, 0.6, 0.6, 0.6]
            };
            return Ok(usize::from(
                interval(self.profile.loot_quality_mod, self.random)?
                    < f64::from(chances[(tier - 1) as usize]),
            ));
        }
        if matches!(roll.item_type, 3 | 4 | 7) {
            if interval(self.profile.loot_quality_mod, self.random)? >= f64::from(0.1f32) {
                return Ok(1);
            }
            if tier < 6 {
                return Ok(2);
            }
            return Ok(
                if interval(self.profile.loot_quality_mod * 0.1, self.random)?
                    >= f64::from(0.1f32 * 0.5)
                {
                    2
                } else {
                    3
                },
            );
        }
        Ok(1)
    }
}
pub(crate) fn interval<R: TreasureRandom>(
    quality: f32,
    random: &mut R,
) -> Result<f64, TreasureError> {
    Ok((unit(random)? - f64::from(quality)).max(0.0))
}
pub(crate) fn spell_levels(id: i32) -> Result<&'static [i64], TreasureError> {
    ace_tables::spell_levels(i64::from(id))
        .ok_or(TreasureError::MissingAsset("spell progression", id as u32))
}
fn spell_selection_code(w: &WeenieV1, roll: &TreasureRoll) -> Result<i32, TreasureError> {
    let armor = p::int(w, "ArmorLevel")?.unwrap_or(0) > 0;
    let damage = p::int(w, "DamageType")?.unwrap_or(0);
    let weapon = p::int(w, "WeaponSkill")?.unwrap_or(0);
    let shield = p::int(w, "CombatUse")? == Some(en("CombatUse", "Shield")?);
    if w.weenie_type == en("WeenieType", "Gem")? as u32 {
        return Ok(1);
    }
    if roll.item_type == 3 {
        return Ok(if armor { 3 } else { 2 });
    }
    if roll.wcid == en("WeenieClassName", "orb")? as u32 {
        return Ok(4);
    }
    if roll.is_caster() && damage != en("DamageType", "Nether")? {
        return Ok(5);
    }
    if roll.is_melee() && weapon != en("Skill", "TwoHandedCombat")? {
        return Ok(6);
    }
    if (roll.is_armor() || roll.item_type == 7) && !shield {
        return clothing_code(w, roll);
    }
    if shield {
        return Ok(8);
    }
    if roll.item_type == 4 {
        return Ok(
            if roll.wcid == en("WeenieClassName", "flasksimple")? as u32 {
                0
            } else {
                16
            },
        );
    }
    if roll.is_missile() || weapon == en("Skill", "TwoHandedCombat")? {
        return Ok(17);
    }
    if roll.is_caster() && damage == en("DamageType", "Nether")? {
        return Ok(19);
    }
    Ok(0)
}
fn clothing_code(w: &WeenieV1, roll: &TreasureRoll) -> Result<i32, TreasureError> {
    if roll.wcid == en("WeenieClassName", "glovescloth")? as u32 {
        return Ok(14);
    }
    if roll.wcid == en("WeenieClassName", "capleather")? as u32 {
        return Ok(20);
    }
    let c = p::int(w, "ClothingPriority")?.unwrap_or(0) as u32;
    if c & 0x3c00 != 0 && c & 0x200 == 0 {
        return Ok(7);
    }
    if c == 0x8000 && roll.is_armor() {
        return Ok(9);
    }
    if c == 0x4000 && roll.base_armor_level > 20 {
        return Ok(10);
    }
    if c & 0x10000 != 0 && roll.base_armor_level > 20 {
        return Ok(11);
    }
    if c & 0x7e != 0 {
        return Ok(12);
    }
    if c == 0x4000 && !roll.is_armor() {
        return Ok(13);
    }
    if c == 0x8000 && !roll.is_armor() {
        return Ok(14);
    }
    if c & 0x300 != 0 {
        return Ok(15);
    }
    if c == 0x10000 {
        return Ok(18);
    }
    Ok(0)
}
#[cfg(test)]
#[path = "treasure_spell_tests.rs"]
mod tests;
