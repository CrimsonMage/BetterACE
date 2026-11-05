use super::PhysicalError;
use bace_gameplay_api::weapon_combat::PhysicalKind;

/// Pinned ACE Player_Combat.GetAttackStamina, using StaminaTable's descending
/// burden denominations for both melee power and missile accuracy.
pub struct AttackStaminaInput {
    pub kind: PhysicalKind,
    pub player: bool,
    pub base_endurance: u32,
    pub weapon_burden: u32,
    pub shield_burden: u32,
    pub shield_placement: bool,
    pub power: f32,
    pub roll: f64,
}

pub fn attack_stamina(input: AttackStaminaInput) -> Result<u32, PhysicalError> {
    let AttackStaminaInput {
        kind: _,
        player,
        base_endurance,
        weapon_burden,
        shield_burden,
        shield_placement: _,
        power,
        roll,
    } = input;
    if !power.is_finite()
        || !(0.0..=1.0).contains(&power)
        || !roll.is_finite()
        || !(0.0..1.0).contains(&roll)
        || base_endurance > 65535
    {
        return Err(PhysicalError::InvalidInput);
    }
    // At this pin, Monster_Melee and Creature_Missile do not spend attack
    // stamina. The player-only cost is charged once per swing or launch.
    if !player {
        return Ok(0);
    }
    let burden = weapon_burden
        .checked_add(shield_burden)
        .ok_or(PhysicalError::Overflow)?;
    let mut burden = i32::try_from(burden).map_err(|_| PhysicalError::Overflow)?;
    let costs = if power < 0.33 {
        [1.5_f32, 1.0, 1.0]
    } else if power < 0.66 {
        [3.0, 2.0, 1.0]
    } else {
        [6.0, 4.0, 2.0]
    };
    let mut base = 0.0_f32;
    for (denomination, cost) in [(1600, costs[0]), (1200, costs[1]), (700, costs[2])] {
        let count = burden / denomination;
        base += cost * count as f32;
        burden -= denomination * count;
    }
    let modifier = (1.0_f32 - (base_endurance as f32 - 50.0) / 480.0).clamp(0.5, 1.0);
    Ok((base * modifier).max(1.0).round_ties_even() as u32)
}

/// Pinned ACE Player_Combat.OnEvade/OnDamage: noncombat players and creatures
/// lose no stamina, and trained defense can avoid the one-point evade cost.
pub fn defense_stamina(
    player: bool,
    in_combat: bool,
    evaded: bool,
    advancement: u32,
    endurance: u32,
    roll: f64,
) -> Result<u32, PhysicalError> {
    if !roll.is_finite() || !(0.0..1.0).contains(&roll) || endurance > 65535 {
        return Err(PhysicalError::InvalidInput);
    }
    if !player || !in_combat {
        return Ok(0);
    }
    if evaded && advancement >= 2 {
        let e = endurance as f32;
        let chance = ((e * e * 0.000005) + (e * 0.00124) - 0.07).clamp(0.0, 0.75);
        Ok(u32::from(f64::from(chance) <= roll))
    } else {
        Ok(1)
    }
}
