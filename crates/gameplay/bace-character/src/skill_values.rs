//! Pinned ACE CreatureSkill.Base/Current and AttributeFormula operation order.
//! Inputs are immutable authoritative attribute/enchantment projections.
use crate::{CharacterProgression, VitalFormula};
use bace_gameplay_api::{ProgressionTarget, SkillAdvancement, TraitDetails};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkillBonuses {
    pub all_skills: u32,
    pub skilled_melee: u32,
    pub skilled_missile: u32,
    pub skilled_magic: u32,
    pub enlightenment: u32,
    pub jack_of_all_trades: u32,
    pub specialized_luminance: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkillValueInputs {
    pub formula: VitalFormula,
    /// DAT MinLevel=1 permits use of an otherwise Untrained skill.
    pub usable_untrained: bool,
    /// Numeric attribute order: Strength, Endurance, Quickness, Coordination,
    /// Focus, Self. These are already authoritative base/current values.
    pub base_attributes: [u32; 6],
    pub current_attributes: [u32; 6],
    pub bonuses: SkillBonuses,
    pub multiplier: f32,
    pub vitae: f32,
    pub additive: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillValues {
    pub skill: u32,
    pub advancement: SkillAdvancement,
    pub base: u32,
    pub current: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillValueError {
    MissingSkill,
    MissingDetails,
    InvalidFormula,
    InvalidModifier,
    Overflow,
}
impl CharacterProgression {
    pub fn skill_values(
        &self,
        skill: u32,
        input: SkillValueInputs,
    ) -> Result<SkillValues, SkillValueError> {
        let projection = self
            .projection(ProgressionTarget::Skill(skill))
            .ok_or(SkillValueError::MissingSkill)?;
        let Some(TraitDetails::Skill { initial_level, .. }) = projection.details else {
            return Err(SkillValueError::MissingDetails);
        };
        project_skill_values(
            skill,
            projection.advancement,
            projection.ranks,
            initial_level,
            input,
        )
    }
}
pub fn project_skill_values(
    skill: u32,
    advancement: SkillAdvancement,
    ranks: u16,
    initial_level: u32,
    input: SkillValueInputs,
) -> Result<SkillValues, SkillValueError> {
    if !input.multiplier.is_finite()
        || input.multiplier < 0.0
        || !input.vitae.is_finite()
        || !(0.0..=1.0).contains(&input.vitae)
    {
        return Err(SkillValueError::InvalidModifier);
    }
    let usable = (advancement as u32) >= SkillAdvancement::Trained as u32
        || (advancement == SkillAdvancement::Untrained && input.usable_untrained);
    let b = input.bonuses;
    let mastery = if [1, 4, 5, 9, 10, 11, 13, 41, 44, 45, 46, 49].contains(&skill) {
        b.skilled_melee
    } else if [2, 3, 8, 12, 47].contains(&skill) {
        b.skilled_missile
    } else if [31, 32, 33, 34, 43].contains(&skill) {
        b.skilled_magic
    } else {
        0
    };
    let shared = u64::from(initial_level)
        + u64::from(ranks)
        + u64::from(b.all_skills)
        + u64::from(mastery) * 10
        + if (advancement as u32) >= SkillAdvancement::Trained as u32 {
            u64::from(b.enlightenment)
        } else {
            0
        };
    let base = shared
        + if usable {
            u64::from(formula(input.formula, input.base_attributes)?)
        } else {
            0
        };
    let current_base = shared
        + if usable {
            u64::from(formula(input.formula, input.current_attributes)?)
        } else {
            0
        };
    let base = u32::try_from(base).map_err(|_| SkillValueError::Overflow)?;
    let current_base = u32::try_from(current_base).map_err(|_| SkillValueError::Overflow)?;
    let post_vitae = u64::from(b.jack_of_all_trades) * 5
        + if advancement == SkillAdvancement::Specialized {
            u64::from(b.specialized_luminance) * 2
        } else {
            0
        };
    let post_vitae = u32::try_from(post_vitae).map_err(|_| SkillValueError::Overflow)?;
    let mut total = current_base as f32 * input.multiplier;
    if input.vitae != 1.0 {
        total *= input.vitae;
    }
    total += post_vitae as f32;
    total += input.additive as f32;
    let total = total.round().max(0.0);
    if !total.is_finite() || f64::from(total) > f64::from(u32::MAX) {
        return Err(SkillValueError::Overflow);
    }
    Ok(SkillValues {
        skill,
        advancement,
        base,
        current: total as u32,
    })
}
pub(crate) fn formula(formula: VitalFormula, values: [u32; 6]) -> Result<u32, SkillValueError> {
    if !formula.enabled {
        return Ok(0);
    }
    if formula.divisor == 0 || !(1..=6).contains(&formula.attribute1) || formula.attribute2 > 6 {
        return Err(SkillValueError::InvalidFormula);
    }
    let mut total = values[formula.attribute1 as usize - 1];
    if formula.attribute2 != 0 {
        total = total
            .checked_add(values[formula.attribute2 as usize - 1])
            .ok_or(SkillValueError::Overflow)?;
    }
    if formula.divisor == 1 {
        return Ok(total);
    }
    let value = (total as f32 / formula.divisor as f32).round();
    if f64::from(value) > f64::from(u32::MAX) {
        return Err(SkillValueError::Overflow);
    }
    Ok(value as u32)
}
