//! ACE Player_Xp.UpdateXpVitae and EnchantmentManager scalar transitions.
//! Registry adoption/two-second delayed removal belong to magic, never a second
//! mutable enchantment owner. Overflow/invalid input rejects before mutation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VitaeXpChange {
    pub death_level: u32,
    pub before_pool: i32,
    pub after_pool: i32,
    pub before_value: f32,
    /// Actual registry value differs slightly from ReduceVitae's normalized return.
    pub after_value: f32,
    pub returned_value: f32,
    pub remove_after_seconds: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VitaeError {
    Invalid,
    Overflow,
    Budget,
}
pub fn vitae_threshold(value: f32, death_level: u32) -> Result<i32, VitaeError> {
    if !value.is_finite() || value < 0.0 || death_level == 0 {
        return Err(VitaeError::Invalid);
    }
    let threshold =
        (f64::from(death_level).powf(2.5) * 2.5 + 20.0) * f64::from(value).powf(5.0) + 0.5;
    if !threshold.is_finite() || threshold < 0.0 || threshold >= f64::from(i32::MAX) {
        return Err(VitaeError::Overflow);
    }
    Ok(threshold as i32)
}
pub fn vitae_experience(
    value: f32,
    pool: i32,
    death_level: u32,
    earned: u64,
) -> Result<VitaeXpChange, VitaeError> {
    if pool < 0 || earned > i64::MAX as u64 {
        return Err(VitaeError::Invalid);
    }
    let mut remaining = i64::from(pool)
        .checked_add(earned as i64)
        .ok_or(VitaeError::Overflow)?;
    let mut raw = value;
    let mut returned = value;
    let mut threshold = vitae_threshold(value, death_level)?;
    let mut reductions = 0;
    while remaining >= i64::from(threshold) {
        if reductions == 128 {
            return Err(VitaeError::Budget);
        }
        reductions += 1;
        remaining -= i64::from(threshold);
        raw += 0.01;
        returned = if (raw - 1.0).abs() < 0.0001 || raw > 1.0 {
            1.0
        } else {
            raw
        };
        if returned == 1.0 {
            break;
        }
        threshold = vitae_threshold(returned, death_level)?;
    }
    Ok(VitaeXpChange {
        death_level,
        before_pool: pool,
        after_pool: i32::try_from(remaining).map_err(|_| VitaeError::Overflow)?,
        before_value: value,
        after_value: raw,
        returned_value: returned,
        remove_after_seconds: ((returned - 1.0).abs() < 0.0001 || returned > 1.0).then_some(2.0),
    })
}
/// InflictVitaePenalty's `amount` argument is unused by the pinned implementation.
/// Configured penalty and maximum are explicit, not wall-clock/global reads.
pub fn inflict_vitae(
    level: u32,
    current: Option<f32>,
    penalty: f64,
    maximum_penalty: f64,
) -> Result<f32, VitaeError> {
    if level == 0
        || !penalty.is_finite()
        || !(0.0..=1.0).contains(&penalty)
        || !maximum_penalty.is_finite()
        || !(0.0..=1.0).contains(&maximum_penalty)
        || current.is_some_and(|v| !v.is_finite() || v < 0.0)
    {
        return Err(VitaeError::Invalid);
    }
    let prop = 1.0 - maximum_penalty;
    let global_max = 100 - (prop * 100.0).round_ties_even() as u32;
    let max_penalty = (level - 1)
        .checked_mul(3)
        .ok_or(VitaeError::Overflow)?
        .max(1)
        .min(global_max);
    let min_vitae = ((100 - max_penalty) as f32 / 100.0).max(prop as f32);
    Ok((current.unwrap_or(1.0) - penalty as f32)
        .max(min_vitae)
        .min(1.0))
}
