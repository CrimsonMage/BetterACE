//! Jewelry, gems and dinnerware mutation pipelines in the pinned source order.
use crate::{
    TreasureError, TreasureRandom, TreasureRoll, death_treasure::MutationContext,
    treasure_properties::*, treasure_tables::en,
};
use bace_content::WeenieV1;
impl<R: TreasureRandom> MutationContext<'_, R> {
    pub(crate) fn mutate_gem(
        &mut self,
        w: &mut WeenieV1,
        magical: bool,
        roll: &mut TreasureRoll,
    ) -> Result<(), TreasureError> {
        self.roll_workmanship(w)?;
        self.color(w)?;
        if magical {
            self.gem_magic(w, roll)?;
            si(w, "UiEffects", en("UiEffects", "Magical")?)?;
            si(w, "ItemUseable", en("Usable", "Contained")?)?;
        } else {
            si(w, "ItemUseable", en("Usable", "No")?)?;
            rd(w, "Spell")?;
            self.clear_magic(w, true)?;
            ri(w, "ItemSkillLevelLimit")?;
        }
        if has_filter(w, "Value")? {
            self.value(w, roll)?
        }
        self.long_desc(w)
    }
    pub(crate) fn mutate_jewelry(
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
        self.gems(w)?;
        self.roll_workmanship(w)?;
        self.wield_level(w)?;
        if magical {
            self.assign_magic(w, roll)?;
        } else {
            self.clear_magic(w, true)?;
        }
        if self.profile.tier == 8 {
            self.gear_rating(w, roll)?;
        }
        self.value(w, roll)?;
        self.long_desc(w)
    }
    pub(crate) fn mutate_dinnerware(
        &mut self,
        w: &mut WeenieV1,
        magical: bool,
        roll: &mut TreasureRoll,
    ) -> Result<(), TreasureError> {
        let material = self.material(w)?;
        si(w, "MaterialType", material)?;
        self.color(w)?;
        self.gems(w)?;
        self.roll_workmanship(w)?;
        if magical && w.weenie_id != en("WeenieClassName", "flasksimple")? as u32 {
            self.assign_magic(w, roll)?;
        }
        if has_filter(w, "Value")? {
            self.value(w, roll)?;
        }
        self.long_desc(w)
    }
}
