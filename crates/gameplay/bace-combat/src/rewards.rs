//! ACE Creature_Death.OnDeath_GrantXP, official 47edade3 (AGPL-3.0-only).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageShare {
    pub actor: u32,
    pub damage: f32,
    pub eligible: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardError {
    Capacity,
    InvalidDamage,
    DuplicateActor,
    OutputCapacity,
    Overflow,
}

/// Build XP awards from an already authoritative damage-history projection.
/// Ineligible attackers remain in the denominator, like ACE. This is neither a
/// death transition nor a durable reward acknowledgement. No mutation on error.
pub fn kill_rewards(
    shares: &[DamageShare],
    xp_override: Option<i32>,
    output: &mut Vec<(u32, i64)>,
) -> Result<(), RewardError> {
    if shares.len() > 256 {
        return Err(RewardError::Capacity);
    }
    // .NET Enumerable.Sum(float) accumulates in double, then casts once.
    let mut sum = 0.0_f64;
    for (index, share) in shares.iter().enumerate() {
        if !share.damage.is_finite() || share.damage < 0.0 {
            return Err(RewardError::InvalidDamage);
        }
        if shares[..index].iter().any(|old| old.actor == share.actor) {
            return Err(RewardError::DuplicateActor);
        }
        sum += f64::from(share.damage);
    }
    let total = sum as f32;
    if !total.is_finite() {
        return Err(RewardError::Overflow);
    }
    if xp_override.is_some_and(|xp| xp < 0) {
        return Err(RewardError::InvalidDamage);
    }
    let count = if total == 0.0 {
        0
    } else {
        shares.iter().filter(|s| s.eligible).count()
    };
    if output.capacity() - output.len() < count {
        return Err(RewardError::OutputCapacity);
    }
    if total == 0.0 {
        return Ok(());
    }
    for share in shares.iter().filter(|s| s.eligible) {
        // Upstream operands are int and float: preserve the f32 multiplication.
        let amount = xp_override.unwrap_or(0) as f32 * (share.damage / total);
        output.push((share.actor, f64::from(amount).round_ties_even() as i64));
    }
    Ok(())
}
