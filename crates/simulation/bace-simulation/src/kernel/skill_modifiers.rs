//! Prepared inputs exclude the live registry. Compose from those stable inputs
//! on each changed revision, so expiration/replacement cannot accumulate buffs.
use crate::{PreparedCharacterSkillInputs, SkillRefreshError};
use bace_magic::{EnchantmentRegistry, enchantment_modifiers};
pub(super) fn attributes(
    character: &bace_character::CharacterProgression,
    base: &PreparedCharacterSkillInputs,
    registry: Option<&EnchantmentRegistry>,
) -> Result<([u32; 6], [u32; 6]), SkillRefreshError> {
    use bace_gameplay_api::{AttributeId, ProgressionTarget, TraitDetails};
    let prepared = compose(base, registry)?;
    let first = prepared
        .inputs
        .first()
        .ok_or(SkillRefreshError::InvalidInput)?
        .1;
    let mut base_values = first.base_attributes;
    let mut current = first.current_attributes;
    let mut count = 0;
    for (index, modifier) in prepared.attribute_modifiers.iter().enumerate() {
        let id =
            AttributeId::try_from(index as u32 + 1).map_err(|_| SkillRefreshError::InvalidInput)?;
        if let Some(value) = character.projection(ProgressionTarget::Attribute(id)) {
            let Some(TraitDetails::Attribute { starting_value }) = value.details else {
                return Err(SkillRefreshError::InvalidInput);
            };
            base_values[index] = starting_value
                .checked_add(u32::from(value.ranks))
                .ok_or(SkillRefreshError::InvalidInput)?;
            current[index] = bace_character::project_attribute_value(
                base_values[index],
                modifier.multiplier,
                modifier.additive,
            )
            .map_err(|_| SkillRefreshError::InvalidInput)?;
            count += 1;
        }
    }
    if count != 0 && count != 6 {
        return Err(SkillRefreshError::InvalidInput);
    }
    Ok((base_values, current))
}
pub(super) fn modifiers(
    registry: &EnchantmentRegistry,
    flags: u32,
    key: u32,
) -> Result<(f32, i32), SkillRefreshError> {
    let multiplier = enchantment_modifiers(registry, flags | 0x4000, key)
        .iter()
        .fold(1.0, |value, e| value * e.spec.value);
    let additive = enchantment_modifiers(registry, flags | 0x8000, key)
        .iter()
        .try_fold(0i32, |value, e| {
            let number = e.spec.value;
            if !number.is_finite()
                || f64::from(number) < f64::from(i32::MIN)
                || f64::from(number) > f64::from(i32::MAX)
            {
                return None;
            }
            value.checked_add(number as i32)
        })
        .ok_or(SkillRefreshError::InvalidInput)?;
    if !multiplier.is_finite() || multiplier < 0.0 {
        return Err(SkillRefreshError::InvalidInput);
    }
    Ok((multiplier, additive))
}
pub(super) fn compose(
    base: &PreparedCharacterSkillInputs,
    registry: Option<&EnchantmentRegistry>,
) -> Result<PreparedCharacterSkillInputs, SkillRefreshError> {
    let mut current = base.clone();
    let Some(registry) = registry else {
        return Ok(current);
    };
    for (index, attribute) in current.attribute_modifiers.iter_mut().enumerate() {
        let (multiplier, additive) = modifiers(registry, 1, index as u32 + 1)?;
        attribute.multiplier *= multiplier;
        attribute.additive = attribute
            .additive
            .checked_add(additive)
            .ok_or(SkillRefreshError::InvalidInput)?;
    }
    let vitae = registry
        .entries()
        .iter()
        .find(|e| e.spell == 666)
        .map_or(1.0, |e| e.spec.value.min(1.0));
    for (skill, input) in &mut current.inputs {
        let (multiplier, additive) = modifiers(registry, 0x10, *skill)?;
        input.multiplier *= multiplier;
        input.vitae *= vitae;
        input.additive = input
            .additive
            .checked_add(additive)
            .and_then(|n| {
                n.checked_add(crate::magic::Magic::prepared_dirty_skill_modifier(
                    registry, *skill,
                ))
            })
            .ok_or(SkillRefreshError::InvalidInput)?;
    }
    Ok(current)
}
