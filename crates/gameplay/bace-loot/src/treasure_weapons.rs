//! Source weapon mutation scripts and their ordered surrounding property mutations.
use crate::{
    TreasureError, TreasureRandom, TreasureRoll, ace_tables, death_treasure::MutationContext,
    treasure_properties::*, treasure_random::unit, treasure_tables::*,
};
use bace_content::WeenieV1;
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn script(&mut self, w: &mut WeenieV1, name: &str) -> Result<(), TreasureError> {
        let scripts = self
            .assets
            .mutation_scripts
            .as_ref()
            .ok_or(TreasureError::MissingAsset("MutationScripts", 0))?;
        scripts.apply(name, w, self.profile.tier, self.random)?;
        Ok(())
    }
    pub(crate) fn mutate_weapon(
        &mut self,
        w: &mut WeenieV1,
        magical: bool,
        roll: &mut TreasureRoll,
    ) -> Result<(), TreasureError> {
        let damage = int(w, "DamageType")?.unwrap_or(0);
        let suffix = if damage != 0 {
            "elemental"
        } else {
            "non_elemental"
        };
        if roll.is_caster() {
            self.script(w, "Casters.caster.txt")?;
            self.script(w, &format!("Casters.caster_{suffix}.txt"))?;
            if int(w, "WieldRequirements")? == Some(en("WieldRequirement", "RawSkill")?) {
                let skill = if damage == en("DamageType", "Nether")? {
                    "VoidMagic"
                } else {
                    "WarMagic"
                };
                si(w, "WieldSkillType", en("Skill", skill)?)?;
            }
            self.script(w, "Casters.weapon_defense.txt")?;
        } else if roll.is_missile() {
            let name = weapon_name(roll.weapon_type)?;
            self.script(w, &format!("MissileWeapons.{name}_{suffix}.txt"))?;
            self.script(w, "MissileWeapons.weapon_defense.txt")?;
        } else if roll.is_melee() {
            let skill = int(w, "WeaponSkill")?.unwrap_or(0);
            let skill = if skill == en("Skill", "HeavyWeapons")? {
                "heavy"
            } else if skill == en("Skill", "LightWeapons")?
                || skill == en("Skill", "FinesseWeapons")?
            {
                "light_finesse"
            } else if skill == en("Skill", "TwoHandedCombat")? {
                "two_handed"
            } else {
                return Err(TreasureError::InvalidTemplate(w.weenie_id));
            };
            let name = weapon_name(roll.weapon_type)?;
            self.script(
                w,
                &format!("MeleeWeapons.Damage_WieldDifficulty_DamageVariance.{skill}_{name}.txt"),
            )?;
            let short = match roll.weapon_type {
                4 => "dagger",
                10 => "sword",
                18 => "two_handed_axe",
                19 => "two_handed_mace",
                20 => "two_handed_spear",
                21 => "two_handed_sword",
                _ => name,
            };
            self.script(
                w,
                &format!("MeleeWeapons.WeaponOffense_WeaponDefense.{short}_offense_defense.txt"),
            )?;
        } else {
            return Err(TreasureError::InvalidTemplate(w.weenie_id));
        }
        if !roll.is_caster()
            && let Some(old) = int(w, "WeaponTime")?
        {
            let quality = self.quality(false)?;
            let modifier = if quality == 0.0 {
                1.0
            } else {
                let rng = (f64::from(-0.025f32)
                    + unit(self.random)? * f64::from(0.025f32 - (-0.025f32)))
                    as f32;
                1.0 - (quality * 0.025 + rng)
            };
            si(w, "WeaponTime", number(f64::from(old as f32 * modifier))?)?;
        }
        let material = self.material(w)?;
        if material > 0 {
            si(w, "MaterialType", material)?;
        }
        self.color(w)?;
        self.gems(w)?;
        self.roll_workmanship(w)?;
        if !roll.is_caster() {
            self.burden(w, true)?;
        }
        self.defense(w, "WeaponMissileDefense")?;
        self.defense(w, "WeaponMagicDefense")?;
        if magical {
            if roll.is_caster() {
                self.caster_spell(w)?;
            }
            self.assign_magic(w, roll)?;
        } else {
            self.clear_magic(w, roll.is_missile())?;
        }
        self.value(w, roll)?;
        self.long_desc(w)
    }
    fn defense(&mut self, w: &mut WeenieV1, name: &str) -> Result<(), TreasureError> {
        if unit(self.random)? >= f64::from(0.1f32) {
            return rf(w, name);
        }
        let field = if self.profile.tier < 7 {
            "T1_T6_Defense"
        } else {
            "T7_T8_Defense"
        };
        let rows = ace_tables::lookup_float("MissileMagicDefense", field)
            .ok_or_else(|| missing("MissileMagicDefense", field))?;
        let draw = unit(self.random)?;
        let mut sum = 0.0f32;
        let mut value = 1.0;
        for &(v, p) in rows {
            sum += p;
            if p > 0.0 {
                value = v;
            }
            if draw < f64::from(sum) {
                value = v;
                break;
            }
        }
        sf(w, name, f64::from(value))
    }
    fn caster_spell(&mut self, w: &mut WeenieV1) -> Result<(), TreasureError> {
        let orb = w.weenie_id == en("WeenieClassName", "W_ORB_CLASS")? as u32;
        let field = if orb {
            "orbSpells"
        } else if int(w, "DamageType")? == Some(en("DamageType", "Nether")?) {
            "netherSpells"
        } else {
            "wandStaffSpells"
        };
        let first = roll("CasterSlotSpells", field, 0.0, self.random)?;
        let levels = ace_tables::spell_levels(i64::from(first))
            .filter(|v| v.len() == 8)
            .ok_or(TreasureError::MissingAsset(
                "SpellProgression",
                first as u32,
            ))?;
        let level = indexed(
            "SpellLevelChance",
            "spellLevelChances",
            (self.profile.tier - 1) as usize,
            0.0,
            self.random,
        )?;
        let id = *levels
            .get((level - 1) as usize)
            .ok_or(TreasureError::Bounds)? as u32;
        sd(w, "Spell", id)?;
        let spell = self.spell(id)?;
        si(
            w,
            "ItemManaCost",
            number(f64::from(
                spell.base_mana as f32 * if orb { 5.0 } else { 2.5 },
            ))?,
        )?;
        si(
            w,
            "ItemUseable",
            en("Usable", "SourceWieldedTargetRemoteNeverWalk")?,
        )
    }
}
fn weapon_name(kind: i32) -> Result<&'static str, TreasureError> {
    match kind {
        2 => Ok("axe"),
        3 => Ok("dagger"),
        4 => Ok("dagger_ms"),
        5 => Ok("mace"),
        6 => Ok("mace_jitte"),
        7 | 20 => Ok("spear"),
        8 => Ok("staff"),
        9 => Ok("sword"),
        10 => Ok("sword_ms"),
        11 => Ok("unarmed"),
        18 | 19 | 21 => Ok("cleaver"),
        13 => Ok("bow"),
        14 => Ok("crossbow"),
        15 => Ok("atlatl"),
        16 => Ok("caster"),
        _ => Err(TreasureError::Bounds),
    }
}
