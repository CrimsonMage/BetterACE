use super::PhysicalError;
use bace_gameplay_api::weapon_combat::PhysicalKind;
/// GDLE MeleeAttackEventData::HandleAttackHook / MissileAttackEventData::FireMissile.
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
        kind,
        player,
        base_endurance,
        weapon_burden,
        shield_burden,
        shield_placement,
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
    let mut cost = match kind {
        PhysicalKind::Melee => {
            let w = weapon_burden as f32 * (1.0 / 450.0);
            let s = shield_burden as f32
                * if shield_placement {
                    1.0 / 680.0
                } else {
                    1.0 / 450.0
                };
            let factor = (0.25 + f64::from(power) * 0.75) as f32;
            ((2.0 + w + s) * factor.min(1.0) + 0.5) as u32
        }
        PhysicalKind::Missile => {
            let burden = weapon_burden
                .checked_add(shield_burden)
                .ok_or(PhysicalError::Overflow)? as f32;
            let divisor = if power < 0.33 {
                900.0
            } else if power < 0.66 {
                600.0
            } else {
                300.0
            };
            (burden / divisor).round() as u32
        }
    }
    .max(1);
    if player {
        match kind {
            PhysicalKind::Melee if base_endurance >= 50 => {
                let e = base_endurance as f32;
                let factor =
                    ((f64::from(e * e) * -0.000003175) - (f64::from(e) * 0.0008889) + 1.052) as f32;
                cost = (f64::from(cost as f32 * factor.clamp(0.5, 1.0)) + roll) as u32;
            }
            PhysicalKind::Missile => {
                let factor = (1.0 - (base_endurance as f32 - 100.0) / 600.0).clamp(0.5, 1.0);
                cost = (cost as f32 * factor).round() as u32;
            }
            _ => {}
        }
    }
    Ok(cost.max(1))
}
/// Successful player evasion spends no point only on the source endurance roll.
pub fn defense_stamina(
    player: bool,
    evaded: bool,
    advancement: u32,
    endurance: u32,
    roll: f64,
) -> Result<u32, PhysicalError> {
    if !roll.is_finite() || !(0.0..1.0).contains(&roll) || endurance > 65535 {
        return Err(PhysicalError::InvalidInput);
    }
    if player && evaded && advancement >= 2 {
        let e = endurance as f32;
        let chance = if endurance >= 50 {
            ((f64::from(e * e) * 0.000005) + (f64::from(e) * 0.00124) - 0.07) as f32
        } else {
            0.0
        };
        Ok(u32::from(roll > f64::from(chance.min(0.75))))
    } else {
        Ok(1)
    }
}
