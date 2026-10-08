use super::PhysicalError;
use bace_gameplay_api::weapon_combat::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectedPhysicalAttack {
    pub hand: PhysicalHand,
    pub maneuver: usize,
    pub attack_type: u32,
    pub skill: u32,
    pub skill_level: u32,
    pub speed: f32,
}
pub fn validate_physical_profile(p: &PhysicalCombatProfile) -> Result<(), PhysicalError> {
    if p.equipment.len() > 64
        || p.equipment.iter().any(|e| e.entity == 0 || e.location == 0)
        || p.equipment.windows(2).any(|e| e[0].entity >= e[1].entity)
    {
        return Err(PhysicalError::InvalidInput);
    }
    if !p.height.is_finite()
        || p.height <= 0.0
        || p.height > 100.0
        || p.skills.len() > 256
        || p.player && p.maneuvers.is_empty()
        || p.maneuvers.len() > 4096
        || p.armor.is_empty()
        || p.armor.len() > 256
        || p.body_attacks.len() > 256
        || !p.range.is_finite()
        || p.range < 0.0
        || p.range == 0.0 && !p.maneuvers.is_empty()
        || p.range > 192.0
    {
        return Err(PhysicalError::InvalidInput);
    }
    if ![p.melee_defense_modifier, p.missile_defense_modifier]
        .into_iter()
        .all(|x| x.is_finite() && (0.0..=100.0).contains(&x))
    {
        return Err(PhysicalError::InvalidInput);
    }
    if let Some(m) = p.missile
        && (!m.speed.is_finite()
            || m.speed <= 0.0
            || m.speed > 1000.0
            || !m.radius.is_finite()
            || m.radius <= 0.0
            || m.radius > 20.0
            || !m.launch_seconds.is_finite()
            || m.launch_seconds < 0.0
            || !m.duration_seconds.is_finite()
            || m.duration_seconds <= 0.0
            || m.duration_seconds < m.launch_seconds
            || m.duration_seconds > 120.0
            || !m.damage_modifier.is_finite()
            || m.damage_modifier < 0.0)
    {
        return Err(PhysicalError::InvalidInput);
    }
    for m in &p.maneuvers {
        if m.hooks.is_empty()
            || m.hooks.len() > 32
            || !m.duration.is_finite()
            || m.duration <= 0.0
            || m.duration > 120.0
            || m.hooks
                .iter()
                .any(|h| !h.seconds.is_finite() || h.seconds < 0.0 || h.seconds > m.duration)
            || m.hooks.windows(2).any(|h| h[0].seconds > h[1].seconds)
        {
            return Err(PhysicalError::InvalidInput);
        }
    }
    for w in [
        &p.main,
        &p.offhand,
        &p.launcher,
        &p.ammunition,
        &p.gloves,
        &p.boots,
    ]
    .into_iter()
    .flatten()
    {
        if ![
            w.damage,
            w.offense,
            w.biting,
            w.crushing,
            w.slayer_bonus,
            w.proc_chance,
        ]
        .into_iter()
        .all(|v| v.is_finite() && v >= 0.0)
            || !w.variance.is_finite()
            || !(0.0..=1.0).contains(&w.variance)
            || w.cleave_targets > 32
            || w.biting > 1.0
            || w.proc_chance > 1.0
            || w.damage > i32::MAX as f64
        {
            return Err(PhysicalError::InvalidInput);
        }
    }
    if p.body_attacks.iter().any(|b| {
        !b.damage.is_finite()
            || b.damage < 0.0
            || !b.variance.is_finite()
            || !(0.0..=1.0).contains(&b.variance)
    }) {
        return Err(PhysicalError::InvalidInput);
    }
    for b in &p.armor {
        if b.hit_weights.iter().any(|v| !v.is_finite() || *v < 0.0)
            || b.armor.iter().any(|v| !super::armor::valid_quality(*v))
        {
            return Err(PhysicalError::InvalidInput);
        }
    }
    if p.armor.windows(2).any(|v| v[0].part >= v[1].part)
        || p.skills.windows(2).any(|v| v[0].0 >= v[1].0)
        || p.skills
            .iter()
            .any(|(_, s)| s.current > i32::MAX as u32 || s.advancement > 3)
    {
        return Err(PhysicalError::InvalidInput);
    }
    if p.armor_layers.len() > 64
        || !p.ignore_shield.is_finite()
        || !(0.0..=1.0).contains(&p.ignore_shield)
        || p.armor_layers.iter().chain(p.shield.iter()).any(|l| {
            !super::armor::valid_quality(l.armor)
                || l.modifiers.iter().any(|q| !super::armor::valid_quality(*q))
        })
    {
        return Err(PhysicalError::InvalidInput);
    }
    if p.resistances
        .iter()
        .any(|r| !super::armor::valid_quality(r.quality) || r.augmentation > 10)
    {
        return Err(PhysicalError::InvalidInput);
    }
    Ok(())
}
pub(super) fn skill(p: &PhysicalCombatProfile, id: u32) -> PhysicalSkill {
    p.skills
        .iter()
        .find(|v| v.0 == id)
        .map(|v| v.1)
        .unwrap_or(PhysicalSkill {
            advancement: 0,
            current: 0,
        })
}
/// CMeleeAttackEvent::Setup and CDualWieldAttackEvent::Setup. The caller owns
/// alternating hands only after successful animation completion, not each hook.
pub fn select_melee(
    p: &PhysicalCombatProfile,
    height: u32,
    power: f32,
    offhand: bool,
) -> Result<SelectedPhysicalAttack, PhysicalError> {
    if !(1..=3).contains(&height) || !power.is_finite() || !(0.0..=1.0).contains(&power) {
        return Err(PhysicalError::InvalidInput);
    }
    let hand = if offhand && p.style == 0x80000046 {
        PhysicalHand::Offhand
    } else if p.main.is_some() {
        PhysicalHand::Main
    } else {
        PhysicalHand::Unarmed
    };
    let weapon = match hand {
        PhysicalHand::Main => p.main.as_ref(),
        PhysicalHand::Offhand => Some(p.offhand.as_ref().ok_or(PhysicalError::MissingWeapon)?),
        PhysicalHand::Unarmed => None,
    };
    let mut id = weapon.map_or(45, |w| w.skill);
    let mut level = skill(p, id).current;
    if hand == PhysicalHand::Offhand && skill(p, 49).current < level {
        id = 49;
        level = skill(p, 49).current;
    }
    let offense = if p.style == 0x80000046 {
        p.main
            .as_ref()
            .map_or(1.0, |w| w.offense)
            .max(p.offhand.as_ref().map_or(1.0, |w| w.offense))
    } else {
        weapon.map_or(1.0, |w| w.offense)
    };
    let effective = f64::from(level) * offense;
    if effective > f64::from(i32::MAX) {
        return Err(PhysicalError::Overflow);
    }
    level = effective as u32;
    let mut attack_type = weapon.map_or(if power >= 0.75 { 8 } else { 1 }, |w| w.attack_type);
    if attack_type == 6 {
        attack_type = if power >= 0.25 { 4 } else { 2 };
    }
    if p.style == 0x80000046 {
        attack_type = match attack_type {
            0xa0 => {
                if power >= 0.25 {
                    0x20
                } else {
                    0x80
                }
            }
            0x140 => {
                if power >= 0.25 {
                    0x40
                } else {
                    0x100
                }
            }
            v => v,
        };
    } else if p.style == 0x80000040 {
        attack_type = match attack_type {
            0xa0 => 0x80,
            0x140 => 0x100,
            v => v,
        };
    }
    if hand == PhysicalHand::Offhand {
        attack_type = match attack_type {
            1 => 0x10,
            8 => 0x19,
            2 => 0x200,
            4 => 0x400,
            0x20 => 0x800,
            0x40 => 0x1000,
            0x80 => 0x2000,
            0x100 => 0x4000,
            v => v,
        };
    }
    let authored = p
        .maneuvers
        .iter()
        .position(|m| m.style == p.style && m.attack_type == attack_type && m.height == height);
    let mut motion = authored.map(|i| p.maneuvers[i].motion);
    if hand != PhysicalHand::Offhand
        && weapon.is_some_and(|w| w.style == 1)
        && power <= 0.25
        && height == 3
    {
        motion = motion.and_then(|m| m.checked_sub(3));
    }
    if motion.is_none() {
        motion = Some(fallback_melee_motion(
            height,
            power,
            hand == PhysicalHand::Offhand,
        )?);
    }
    let maneuver = p
        .maneuvers
        .iter()
        .position(|m| Some(m.motion) == motion)
        .ok_or(PhysicalError::MissingMotion)?;
    let speed = attack_speed(p.quickness, weapon.map_or(0, |w| w.attack_time))?;
    Ok(SelectedPhysicalAttack {
        hand,
        maneuver,
        attack_type,
        skill: id,
        skill_level: level,
        speed,
    })
}

/// CMeleeAttackEvent::Setup: integer attack-time averaging precedes float speed.
pub fn attack_speed(quickness: u32, weapon_time: i32) -> Result<f32, PhysicalError> {
    let q = i32::try_from(quickness).map_err(|_| PhysicalError::Overflow)?;
    let w = weapon_time;
    let creature = 120i32
        .checked_sub((q - 60) / 2)
        .ok_or(PhysicalError::Overflow)?
        .max(0);
    let time = (creature.checked_add(w).ok_or(PhysicalError::Overflow)? / 2).clamp(0, 120);
    Ok(((2.25 - f64::from(time) * (1.0 / 70.0)) as f32).clamp(0.8, 2.25))
}

/// CMissileAttackEvent::Setup uses a 2.5 upper/base speed (melee uses 2.25).
pub fn missile_attack_speed(quickness: u32, weapon_time: i32) -> Result<f32, PhysicalError> {
    let q = i32::try_from(quickness).map_err(|_| PhysicalError::Overflow)?;
    let creature = 120i32
        .checked_sub((q - 60) / 2)
        .ok_or(PhysicalError::Overflow)?
        .max(0);
    let time = (creature
        .checked_add(weapon_time)
        .ok_or(PhysicalError::Overflow)?
        / 2)
    .clamp(0, 120);
    Ok(((2.5 - f64::from(time) * (1.0 / 70.0)) as f32).clamp(0.8, 2.5))
}

/// GDLE CMeleeAttackEvent::Setup has this explicit no-CMT/no-row fallback.
pub fn fallback_melee_motion(height: u32, power: f32, offhand: bool) -> Result<u32, PhysicalError> {
    if !(1..=3).contains(&height) || !power.is_finite() || !(0.0..=1.0).contains(&power) {
        return Err(PhysicalError::InvalidInput);
    }
    Ok((if offhand { 0x10000185 } else { 0x10000061 })
        + height
        + if power >= 0.75 {
            6
        } else if power >= 0.25 {
            3
        } else {
            0
        })
}
