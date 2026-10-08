//! Pinned ACE cantrip count, unique selection, progression and requirement changes.
use crate::{
    TreasureError, TreasureRandom, TreasureRoll,
    death_treasure::MutationContext,
    treasure_properties as p,
    treasure_random::unit,
    treasure_spells::spell_levels,
    treasure_tables::{en, indexed, roll as table_roll},
};
use bace_content::WeenieV1;
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn cantrips(
        &mut self,
        w: &mut WeenieV1,
        roll: &TreasureRoll,
    ) -> Result<Vec<i32>, TreasureError> {
        if roll.item_type == 4 {
            return Ok(vec![]);
        }
        let count = indexed(
            "CantripChance",
            "_numCantrips",
            (self.profile.tier - 1) as usize,
            self.profile.loot_quality_mod,
            self.random,
        )?;
        if !(0..=5).contains(&count) {
            return Err(TreasureError::Bounds);
        }
        let mut selected = Vec::new();
        for _ in 0..count * 3 {
            if selected.len() >= count as usize {
                break;
            }
            let spell = self.cantrip(w, roll)?;
            if spell != 0 && !selected.contains(&spell) {
                selected.push(spell);
            }
        }
        let mut result = Vec::new();
        let mut legendary = false;
        for base in selected {
            let level = indexed(
                "CantripChance",
                "_cantripLevels",
                (self.profile.tier - 1) as usize,
                self.profile.loot_quality_mod,
                self.random,
            )?;
            let levels = spell_levels(base)?;
            if levels.len() != 4 {
                return Err(TreasureError::MissingAsset(
                    "cantrip progression",
                    base as u32,
                ));
            }
            let spell = *levels
                .get((level - 1) as usize)
                .ok_or(TreasureError::Bounds)?;
            result.push(i32::try_from(spell).map_err(|_| TreasureError::Bounds)?);
            legendary |= level == 4;
        }
        if legendary && roll.armor_type != en("TreasureArmorType", "Society")? {
            let requirement = en("WieldRequirement", "Level")?;
            for (kind, value) in [
                ("WieldRequirements", "WieldDifficulty"),
                ("WieldRequirements2", "WieldDifficulty2"),
            ] {
                if p::int(w, kind)? == Some(requirement)
                    && p::int(w, value)?.is_some_and(|v| v < 180)
                {
                    p::si(w, value, 180)?;
                }
            }
        }
        Ok(result)
    }
    fn cantrip(&mut self, w: &WeenieV1, roll: &TreasureRoll) -> Result<i32, TreasureError> {
        let (class, field) = if p::int(w, "ArmorLevel")?.unwrap_or(0) > 0 || roll.item_type == 7 {
            ("ArmorCantrips", "armorCantrips")
        } else if roll.is_melee() {
            ("MeleeCantrips", "meleeCantrips")
        } else if roll.is_missile() {
            ("MissileCantrips", "missileCantrips")
        } else if roll.is_caster() {
            ("WandCantrips", "casterCantrips")
        } else if roll.item_type == 3 {
            ("JewelryCantrips", "jewelryCantrips")
        } else {
            return Ok(0);
        };
        let spell = table_roll(class, field, 0.0, self.random)?;
        if roll.is_melee()
            && class == "MeleeCantrips"
            && spell == en("SpellId", "CANTRIPLIGHTWEAPONSAPTITUDE1")?
        {
            return self.adjust_mastery(w);
        }
        if roll.is_caster()
            && class == "WandCantrips"
            && spell == en("SpellId", "CANTRIPWARMAGICAPTITUDE1")?
        {
            let damage = p::int(w, "DamageType")?.unwrap_or(0);
            if damage == en("DamageType", "Nether")? {
                return en("SpellId", "CantripVoidMagicAptitude1");
            }
            if damage != 0 || unit(self.random)? < 0.5 {
                return en("SpellId", "CANTRIPWARMAGICAPTITUDE1");
            }
            return en("SpellId", "CantripVoidMagicAptitude1");
        }
        Ok(spell)
    }
    fn adjust_mastery(&mut self, w: &WeenieV1) -> Result<i32, TreasureError> {
        let skill = p::int(w, "WeaponSkill")?.unwrap_or(0);
        if skill == en("Skill", "TwoHandedCombat")? {
            return en("SpellId", "CANTRIPTWOHANDEDAPTITUDE1");
        }
        if unit(self.random)? < f64::from(0.1f32) {
            return en("SpellId", "CantripDualWieldAptitude1");
        }
        for (name, spell) in [
            ("HeavyWeapons", "CANTRIPHEAVYWEAPONSAPTITUDE1"),
            ("LightWeapons", "CANTRIPLIGHTWEAPONSAPTITUDE1"),
            ("FinesseWeapons", "CANTRIPFINESSEWEAPONSAPTITUDE1"),
        ] {
            if skill == en("Skill", name)? {
                return en("SpellId", spell);
            }
        }
        Ok(0)
    }
}
