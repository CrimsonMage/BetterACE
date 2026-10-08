//! ACE SkillCheck.GetSkillChance, official 47edade3 (AGPL-3.0-only).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillError {
    DifferenceOverflow,
}

/// Scalar ACE logistic skill contest, retaining the upstream f32 product before
/// promotion to the double-precision exponential. Invalid overflow is rejected.
pub fn skill_chance(skill: i32, difficulty: i32) -> Result<f64, SkillError> {
    let difference = skill
        .checked_sub(difficulty)
        .ok_or(SkillError::DifferenceOverflow)?;
    let exponent = 0.03_f32 * difference as f32;
    Ok((1.0 - 1.0 / (1.0 + f64::from(exponent).exp())).clamp(0.0, 1.0))
}
