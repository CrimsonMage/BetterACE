//! Pinned GDLE Enchantment::Enchant(float*/EnchantedQualityDetails*) arithmetic.
//! Registry winner selection remains the accepted ACE registry policy.
use crate::{EffectError, EnchantmentRegistry, enchantment_modifiers};
use bace_gameplay_api::weapon_combat::PhysicalQuality;
pub fn enchant_physical_quality(
    registry: &EnchantmentRegistry,
    family: u32,
    key: u32,
    raw: f64,
    integer: bool,
) -> Result<(PhysicalQuality, f64), EffectError> {
    if ![4, 8, 128].contains(&family) || !raw.is_finite() {
        return Err(EffectError::InvalidState);
    }
    let mut selected = enchantment_modifiers(registry, family, key);
    // GDLE culls _mult_list before _add_list. Within either list order is stable.
    selected.sort_by_key(|entry| entry.spec.stat_type & 0x8000 != 0);
    let mut details = PhysicalQuality {
        raw,
        increasing: 1.,
        decreasing: 1.,
        additive_increasing: 0.,
        additive_decreasing: 0.,
    };
    let mut value = raw as f32;
    for entry in selected {
        let modifier = entry.spec.value;
        if !modifier.is_finite() {
            return Err(EffectError::InvalidState);
        }
        if entry.spec.stat_type & 0x8000 != 0 {
            value += modifier;
            if modifier <= 0. {
                details.additive_decreasing += f64::from(modifier);
            } else {
                details.additive_increasing += f64::from(modifier);
            }
        } else if entry.spec.stat_type & 0x4000 != 0 {
            value *= modifier;
            if modifier <= 1. {
                details.decreasing *= f64::from(modifier);
            } else {
                details.increasing *= f64::from(modifier);
            }
        }
    }
    if integer {
        if value <= 0.5 {
            value = 0.;
        }
        value += 0.5;
    }
    if !value.is_finite()
        || [
            details.increasing,
            details.decreasing,
            details.additive_increasing,
            details.additive_decreasing,
        ]
        .iter()
        .any(|v| !v.is_finite())
    {
        return Err(EffectError::Overflow);
    }
    let value = if integer {
        if f64::from(value) > f64::from(i32::MAX) {
            return Err(EffectError::Overflow);
        }
        f64::from(value as i32)
    } else {
        f64::from(value)
    };
    Ok((details, value))
}

/// GDLE queries undefined, base, then typed body-armor enchantments in that order.
pub fn enchant_body_quality(
    registry: &EnchantmentRegistry,
    damage: u32,
    raw: f64,
) -> Result<PhysicalQuality, EffectError> {
    let mut result = PhysicalQuality {
        raw,
        increasing: 1.,
        decreasing: 1.,
        additive_increasing: 0.,
        additive_decreasing: 0.,
    };
    for key in [0, 0x10000000, damage] {
        let (part, _) = enchant_physical_quality(registry, 128, key, 0., false)?;
        result.increasing *= part.increasing;
        result.decreasing *= part.decreasing;
        result.additive_increasing += part.additive_increasing;
        result.additive_decreasing += part.additive_decreasing;
    }
    if [
        result.increasing,
        result.decreasing,
        result.additive_increasing,
        result.additive_decreasing,
    ]
    .iter()
    .any(|v| !v.is_finite())
    {
        return Err(EffectError::Overflow);
    }
    Ok(result)
}
