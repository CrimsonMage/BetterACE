//! ACE RecipeManager.GetTinkerChance at 47edade3 (AGPL-3.0-only).
//! The 33% / 38% caps are an explicit BetterACE owner-directed correction.
use crate::CraftError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChanceInput {
    pub skill: u32,
    pub trained: bool,
    pub lum_craft: u32,
    pub tool_workmanship: f32,
    pub target_workmanship: f32,
    pub material: u32,
    pub times_tinkered: u32,
    pub imbue: bool,
    pub imbue_augmentation: bool,
    pub foolproof: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TinkerChance {
    pub difficulty: i32,
    pub effective_skill: i32,
    pub probability: f64,
}

/// MaterialType numeric IDs from official ACE, not template IDs.
pub fn material_modifier(material: u32) -> f32 {
    match material {
        0x3c | 0x4b => 10.0,
        0x42 | 0x35 | 0x39 | 0x3a | 1 | 0x43 | 4 | 0x44 | 0x1f | 0x21 | 0x4c | 0x37 | 7 | 8 => 11.0,
        0x49 | 0x17 | 0x3d | 0x4a | 2 | 5 | 0x40 | 0x4d => 12.0,
        0x11 | 0x12 | 0x13 | 0x19 | 0x1d | 0x1e | 0x24 | 0x25 => 25.0,
        _ => 20.0,
    }
}

pub fn tinker_chance(input: ChanceInput) -> Result<TinkerChance, CraftError> {
    const ATTEMPTS: [f32; 10] = [1.0, 1.1, 1.3, 1.6, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5];
    if !input.trained {
        return Err(CraftError::Untrained);
    }
    let attempt = *ATTEMPTS
        .get(input.times_tinkered as usize)
        .ok_or(CraftError::TinkerLimit)?;
    if !input.tool_workmanship.is_finite()
        || !input.target_workmanship.is_finite()
        || input.tool_workmanship <= 0.0
        || input.target_workmanship <= 0.0
    {
        return Err(CraftError::InvalidState);
    }
    let effective_skill = i32::try_from(
        input
            .skill
            .checked_add(input.lum_craft)
            .ok_or(CraftError::Overflow)?,
    )
    .map_err(|_| CraftError::Overflow)?;
    let modifier = material_modifier(input.material);
    let workmanship_modifier = if input.tool_workmanship >= input.target_workmanship {
        2.0
    } else {
        1.0
    };
    let raw = ((modifier * 5.0) + (input.target_workmanship * modifier * 2.0)
        - (input.tool_workmanship * workmanship_modifier * modifier / 5.0))
        * attempt;
    let floor = f64::from(raw).floor();
    if !floor.is_finite() || floor < f64::from(i32::MIN) || floor > f64::from(i32::MAX) {
        return Err(CraftError::Overflow);
    }
    let difficulty = floor as i32;
    let difference = effective_skill
        .checked_sub(difficulty)
        .ok_or(CraftError::Overflow)?;
    let exponent = 0.03_f32 * difference as f32;
    let base = (1.0 - 1.0 / (1.0 + f64::from(exponent).exp())).clamp(0.0, 1.0);
    let probability = if input.foolproof {
        1.0
    } else if input.imbue {
        let bonus = if input.imbue_augmentation { 0.05 } else { 0.0 };
        (base / 3.0 + bonus).min(if input.imbue_augmentation { 0.38 } else { 0.33 })
    } else {
        base
    };
    Ok(TinkerChance {
        difficulty,
        effective_skill,
        probability,
    })
}
