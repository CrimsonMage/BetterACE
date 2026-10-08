//! Mana, spellcraft and activation requirements from LootGenerationFactory_Magic/Gem.
use crate::{
    TreasureError, TreasureRandom, TreasureRoll, TreasureSpell, ace_tables,
    death_treasure::MutationContext,
    treasure_properties::*,
    treasure_random::{inclusive, unit},
    treasure_tables::*,
};
use bace_content::WeenieV1;
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn spell(&self, id: u32) -> Result<TreasureSpell, TreasureError> {
        self.assets
            .spells
            .get(&id)
            .copied()
            .ok_or(TreasureError::MissingAsset("SpellTable", id))
    }
    pub(crate) fn item_mana(
        &mut self,
        w: &mut WeenieV1,
        roll: &TreasureRoll,
        max: i32,
    ) -> Result<(), TreasureError> {
        let (lo, hi) =
            if roll.item_type == 7 || roll.is_armor() || roll.is_weapon() || roll.item_type == 4 {
                (6, 15)
            } else if roll.item_type == 3 {
                (12, 20)
            } else if roll.item_type == 2 {
                (1, 1)
            } else {
                return Err(TreasureError::Bounds);
            };
        let rng = inclusive(self.random, lo, hi)?;
        let mana = number(f64::from(max as f32 * workmanship(w, 1)? * rng as f32).ceil())?;
        si(w, "ItemMaxMana", mana)?;
        si(w, "ItemCurMana", mana)
    }
    pub(crate) fn assign_magic(
        &mut self,
        w: &mut WeenieV1,
        roll: &mut TreasureRoll,
    ) -> Result<(), TreasureError> {
        self.assign_spells(w, roll)?;
        si(w, "UiEffects", en("UiEffects", "Magical")?)?;
        let mut max_mana = 0;
        let mut max_power = 0;
        for p in &w.properties.spell_book {
            let spell = self.spell(p.id as u32)?;
            max_mana = max_mana.max(spell.base_mana);
            max_power = max_power.max(spell.power)
        }
        sf(
            w,
            "ManaRate",
            f64::from(-1.0f32 / (1200.0f32 / max_mana.max(1) as f32).ceil()),
        )?;
        if let Some(id) = did(w, "Spell")? {
            let spell = self.spell(id)?;
            max_mana = max_mana.max(
                spell
                    .base_mana
                    .checked_mul(5)
                    .ok_or(TreasureError::Bounds)?,
            );
            max_power = max_power.max(spell.power)
        }
        self.item_mana(
            w,
            roll,
            i32::try_from(max_mana).map_err(|_| TreasureError::Bounds)?,
        )?;
        let (lo, hi) = if roll.item_type == 7
            || roll.is_armor()
            || roll.is_weapon()
            || matches!(roll.item_type, 3 | 4)
        {
            (0.9f32, 1.1f32)
        } else {
            (1.0, 1.0)
        };
        let rng = f64::from(lo) + unit(self.random)? * f64::from(hi - lo);
        let craft = number((f64::from(max_power) * rng).ceil())?.min(370);
        si(w, "ItemSpellcraft", craft)?;
        let limit = roll.is_melee()
            || roll.is_missile()
            || (roll.is_armor() && unit(self.random)? < f64::from(0.55f32));
        if limit {
            let mut cap = craft.checked_add(20).ok_or(TreasureError::Bounds)?;
            let skill = if roll.is_melee() || roll.is_missile() {
                int(w, "WeaponSkill")?.unwrap_or(0)
            } else if unit(self.random)? < f64::from(0.5f32) {
                en("Skill", "MeleeDefense")?
            } else {
                cap = number(f64::from(cap as f32 * 0.7f32))?;
                en("Skill", "MissileDefense")?
            };
            si(w, "ItemSkillLevelLimit", cap)?;
            sd(w, "ItemSkillLimit", skill as u32)?;
        }
        let factor = int(w, "ItemSkillLevelLimit")?
            .filter(|&v| v > 0)
            .map_or(0.0, |v| v as f32 / 2.0);
        let arcane = (craft as f32 - factor).max(0.0) + roll.item_difficulty;
        si(w, "ItemDifficulty", number(f64::from(arcane).floor())?)
    }
    pub(crate) fn clear_magic(
        &self,
        w: &mut WeenieV1,
        mana_rate: bool,
    ) -> Result<(), TreasureError> {
        for n in [
            "ItemManaCost",
            "ItemMaxMana",
            "ItemCurMana",
            "ItemSpellcraft",
            "ItemDifficulty",
        ] {
            ri(w, n)?
        }
        if mana_rate {
            rf(w, "ManaRate")?
        }
        Ok(())
    }
    pub(crate) fn gem_magic(
        &mut self,
        w: &mut WeenieV1,
        roll: &TreasureRoll,
    ) -> Result<(), TreasureError> {
        let spell = indexed(
            "SpellSelectionTable",
            "spellSelectionGroup",
            0,
            0.0,
            self.random,
        )?;
        let level = indexed(
            "SpellLevelChance",
            "spellLevelChances",
            (self.profile.tier - 1) as usize,
            0.0,
            self.random,
        )?;
        let levels = ace_tables::spell_levels(i64::from(spell))
            .filter(|v| v.len() == 8)
            .ok_or(TreasureError::MissingAsset(
                "SpellProgression",
                spell as u32,
            ))?;
        let id = *levels
            .get((level - 1) as usize)
            .ok_or(TreasureError::Bounds)? as u32;
        sd(w, "Spell", id)?;
        let spell = self.spell(id)?;
        si(w, "ItemSpellcraft", spell.power.min(370) as i32)?;
        let mana = i32::try_from(spell.base_mana)
            .ok()
            .and_then(|v| v.checked_mul(5))
            .ok_or(TreasureError::Bounds)?;
        self.item_mana(w, roll, mana)?;
        si(w, "ItemManaCost", mana)
    }
}
