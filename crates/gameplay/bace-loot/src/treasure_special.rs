//! Pinned ACE cloak, summoning-device and mundane-add-on treasure behavior.
use crate::{
    TreasureError, TreasureRandom, TreasureRoll, ace_tables,
    death_treasure::MutationContext,
    treasure_properties as p,
    treasure_random::{inclusive, unit},
    treasure_tables::{en, indexed, missing, sequence},
};
use bace_content::WeenieV1;
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn mutate_cloak(
        &mut self,
        w: &mut WeenieV1,
        roll: &TreasureRoll,
    ) -> Result<(), TreasureError> {
        let level = indexed(
            "CloakChance",
            "cloakLevels",
            (self.profile.tier - 1) as usize,
            self.profile.loot_quality_mod,
            self.random,
        )?;
        if !(1..=5).contains(&level) {
            return Err(TreasureError::Bounds);
        }
        p::si(w, "ItemMaxLevel", level)?;
        p::si(w, "WieldDifficulty", level * 30)?;
        p::sd(w, "IconOverlay", 0x06006c34 + (level - 1) as u32)?;
        p::si(
            w,
            "EquipmentSetId",
            sequence("CloakChance", "cloakSets", self.random)?,
        )?;
        let spells = ace_tables::sequence("CloakChance", "surgeSpells")
            .ok_or_else(|| missing("CloakChance", "surgeSpells"))?;
        let index = inclusive(self.random, 0, spells.len() as i32)? as usize;
        if let Some(&spell) = spells.get(index) {
            let spell = u32::try_from(spell).map_err(|_| TreasureError::Bounds)?;
            p::sd(w, "ProcSpell", spell)?;
            let self_targeted = spell == en("SpellId", "CloakAllSkill")? as u32;
            p::set(
                &mut w.properties.bools,
                p::id("PropertyBool", "ProcSpellSelfTargeted")?,
                self_targeted,
            );
            p::si(w, "CloakWeaveProc", 1)?;
        } else {
            p::si(w, "CloakWeaveProc", 2)?;
        }
        let material = self.material(w)?;
        p::si(w, "MaterialType", material)?;
        let workmanship = indexed(
            "WorkmanshipChance",
            "workmanshipChances",
            (self.profile.tier.clamp(1, 6) - 1) as usize,
            0.0,
            self.random,
        )?;
        let items = p::int(w, "NumItemsInMaterial")?.unwrap_or(1);
        let stored = f64::from(workmanship as f32 * items as f32).round_ties_even();
        p::si(w, "ItemWorkmanship", p::number(stored)?)?;
        if self.profile.tier == 8 {
            self.gear_rating(w, roll)?;
        }
        self.value(w, roll)
    }
    pub(crate) fn mutate_pet(&mut self, w: &mut WeenieV1) -> Result<(), TreasureError> {
        if w.weenie_type != en("WeenieType", "PetDevice")? as u32 {
            return Ok(());
        }
        for property in [
            "GearDamage",
            "GearDamageResist",
            "GearCritDamage",
            "GearCritDamageResist",
            "GearCrit",
            "GearCritResist",
        ] {
            if unit(self.random)? < 0.5 {
                let rating = self.pet_rating()?;
                p::si(w, property, rating)?;
            }
        }
        self.roll_workmanship(w)
    }
    fn pet_rating(&mut self) -> Result<i32, TreasureError> {
        let mut rating = inclusive(self.random, 1, 10)?;
        let chance = 0.4f32 + self.profile.tier as f32 * 0.02f32;
        if unit(self.random)? < f64::from(chance) {
            rating += inclusive(self.random, 1, 10)?;
        }
        Ok(rating)
    }
    pub(crate) fn mundane_addon(&mut self, rate: f32) -> Result<Option<WeenieV1>, TreasureError> {
        if !rate.is_finite() {
            return Err(TreasureError::Bounds);
        }
        let tier = self.profile.tier;
        if tier <= 4 {
            if unit(self.random)? >= f64::from(0.02f32) {
                return Ok(None);
            }
            let id = indexed(
                "CoalescedManaWcids",
                "tierChances",
                (tier - 1) as usize,
                self.profile.loot_quality_mod,
                self.random,
            )?;
            return Ok(Some(self.template(id as u32)?));
        }
        if rate <= 0.0 {
            return Ok(None);
        }
        let multiplier = 1.0f32 / rate;
        if !multiplier.is_finite() {
            return Err(TreasureError::Bounds);
        }
        if unit(self.random)? * f64::from(multiplier) >= f64::from(0.02f32) {
            return Ok(None);
        }
        let colors = ace_tables::sequence("AetheriaWcids", "aetheriaColors")
            .ok_or_else(|| missing("AetheriaWcids", "aetheriaColors"))?;
        let index = match tier {
            5 => 0,
            6 => inclusive(self.random, 0, 1)? as usize,
            7 | 8 => inclusive(self.random, 0, 2)? as usize,
            _ => return Err(TreasureError::Bounds),
        };
        let id = *colors.get(index).ok_or(TreasureError::Bounds)?;
        let mut w = self.template(u32::try_from(id).map_err(|_| TreasureError::Bounds)?)?;
        let level = indexed(
            "AetheriaChance",
            "itemMaxLevels",
            (tier - 5) as usize,
            self.profile.loot_quality_mod,
            self.random,
        )?;
        if !(1..=5).contains(&level) {
            return Err(TreasureError::Bounds);
        }
        p::si(&mut w, "ItemMaxLevel", level)?;
        p::sd(&mut w, "IconOverlay", 0x06006c34 + (level - 1) as u32)?;
        Ok(Some(w))
    }
}
