//! Pinned ACE CreatureVital.GetMaxValue and CreatureAttribute.GetCurrent.
//! Enchantment selection belongs to magic; this module preserves arithmetic order.
use crate::{SkillValueError, VitalFormula};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VitalValueInputs {
    pub formula: VitalFormula,
    pub starting_value: u32,
    pub ranks: u32,
    pub current_attributes: [u32; 6],
    /// For health only: Enlightenment * 2 + GearMaxHealth, before multipliers.
    pub base_bonus: u32,
    pub multiplier: f32,
    pub vitae: f32,
    pub additive: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VitalValues {
    pub before_multipliers: u32,
    pub maximum: u32,
}
pub fn project_vital_values(input: VitalValueInputs) -> Result<VitalValues, SkillValueError> {
    if !input.multiplier.is_finite()
        || input.multiplier < 0.
        || !input.vitae.is_finite()
        || !(0.0..=1.0).contains(&input.vitae)
        || !input.additive.is_finite()
    {
        return Err(SkillValueError::InvalidModifier);
    }
    let attribute = crate::skill_values::formula(input.formula, input.current_attributes)?;
    let total = input
        .starting_value
        .checked_add(input.ranks)
        .and_then(|v| v.checked_add(attribute))
        .and_then(|v| v.checked_add(input.base_bonus))
        .ok_or(SkillValueError::Overflow)?;
    let mut scaled = total as f32 * input.multiplier;
    if input.vitae != 1. {
        scaled *= input.vitae;
    }
    let value = (scaled + input.additive).round();
    if !value.is_finite()
        || f64::from(value) > f64::from(i32::MAX)
        || f64::from(value) < f64::from(i32::MIN)
    {
        return Err(SkillValueError::Overflow);
    }
    Ok(VitalValues {
        before_multipliers: total,
        maximum: (value as i32).max(if total >= 5 { 5 } else { 1 }) as u32,
    })
}
pub fn project_attribute_value(
    base: u32,
    multiplier: f32,
    additive: i32,
) -> Result<u32, SkillValueError> {
    if !multiplier.is_finite() || multiplier < 0. {
        return Err(SkillValueError::InvalidModifier);
    }
    let base_signed = i32::try_from(base).map_err(|_| SkillValueError::Overflow)?;
    let value = (base_signed as f32 * multiplier + additive as f32).round();
    if !value.is_finite()
        || f64::from(value) > f64::from(i32::MAX)
        || f64::from(value) < f64::from(i32::MIN)
    {
        return Err(SkillValueError::Overflow);
    }
    Ok((value as i32).max(if base >= 10 { 10 } else { 1 }) as u32)
}
