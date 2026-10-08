//! GDLE SpellProjectile::DoCollision -> CombatFormulas -> TakeDamage ordering.
//! Ratings are returned for the combat owner's reviewed negative-rating repair.
use crate::{EffectError as E, MagicDamageProfile, MagicDamageRolls, MagicSchool};
use bace_gameplay_api::weapon_combat::PhysicalQuality;
pub struct MagicDamageInput<'a> {
    pub source: &'a MagicDamageProfile,
    pub target: &'a MagicDamageProfile,
    pub school: MagicSchool,
    pub skill: u32,
    pub formula_level: u32,
    pub damage_type: u32,
    pub minimum: u32,
    pub maximum: u32,
    /// Life-projectile drained amount times authored ratio, before rounding.
    pub life_damage: Option<f64>,
    pub projectile: bool,
    pub target_in_combat: bool,
    pub target_angle_degrees: f64,
    pub rolls: MagicDamageRolls,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MagicDamageResult {
    pub before_rating: f64,
    pub rating: i32,
    pub critical: bool,
    pub critical_defended: bool,
    pub sneak: bool,
    pub sneak_rating: i32,
    pub rending: Option<f64>,
    pub ignore_magic_resistance: bool,
}
pub fn validate_magic_damage_profile(p: &MagicDamageProfile) -> Result<(), E> {
    if p.spellcraft.is_some_and(|v| v > i32::MAX as u32)
        || p.proc_items.len() > 64
        || p.boost_resistances
            .iter()
            .any(|v| !v.is_finite() || *v < 0.)
    {
        return Err(E::InvalidState);
    }
    for (index, item) in p.proc_items.iter().enumerate() {
        if item.item == 0
            || p.proc_items[..index].iter().any(|p| p.item == item.item)
            || item.spellcraft.is_some_and(|v| v > i32::MAX as u32)
            || item.skills.iter().any(|v| *v > i32::MAX as u32)
            || item
                .wield_difficulty
                .is_some_and(|v| v > i32::MAX as u32 / 3)
        {
            return Err(E::InvalidState);
        }
    }
    for (index, sigil) in p.sigils.iter().enumerate() {
        if p.sigils[..index].iter().any(|s| s.slot == sigil.slot) {
            return Err(E::InvalidState);
        }
    }
    if p.sigils.len() > 3
        || p.cloak.is_some_and(|c| c.item == 0 || c.level > 100)
        || p.sigils
            .iter()
            .any(|s| s.item == 0 || s.spell == 0 || s.level > 100 || s.slot > 2)
    {
        return Err(E::InvalidState);
    }
    if p.rating_properties.len() > 32
        || !p.elemental_modifier.is_finite()
        || p.elemental_modifier < 0.0
    {
        return Err(E::InvalidState);
    }
    for (i, property) in p.rating_properties.iter().enumerate() {
        if p.rating_properties[..i]
            .iter()
            .any(|old| old.0 == property.0)
        {
            return Err(E::InvalidState);
        }
    }
    for skill in [p.magic_defense, p.sneak, p.deception, p.assess_person] {
        if skill.advancement > 3 || skill.current > i32::MAX as u32 {
            return Err(E::InvalidState);
        }
    }
    for r in &p.resistances {
        if [
            r.quality.raw,
            r.quality.increasing,
            r.quality.decreasing,
            r.quality.additive_increasing,
            r.quality.additive_decreasing,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || r.quality.raw < 0.0
            || r.quality.increasing < 0.0
            || r.quality.decreasing < 0.0
            || r.augmentation > 10
        {
            return Err(E::InvalidState);
        }
    }
    if let Some((skill, cap)) = p.shield
        && (skill.advancement > 3
            || skill.current > i32::MAX as u32
            || !cap.is_finite()
            || !(0.0..=1.0).contains(&cap))
    {
        return Err(E::InvalidState);
    }
    if let Some(w) = &p.wand
        && (w.entity == 0
            || [w.elemental_modifier, w.biting, w.crushing, w.slayer_bonus]
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0)
            || w.biting > 1.0)
    {
        return Err(E::InvalidState);
    }
    Ok(())
}
fn imbue(skill: f64, min: f64, max: f64, base: f64, cap: f64) -> f64 {
    if skill < min {
        base
    } else if skill > max {
        cap
    } else {
        ((skill - min) / (max - min) * cap).max(0.0).min(cap)
    }
}
pub fn magic_damage_before_mitigation(i: &MagicDamageInput<'_>) -> Result<MagicDamageResult, E> {
    validate_magic_damage_profile(i.source)?;
    validate_magic_damage_profile(i.target)?;
    if i.minimum > i.maximum
        || i.maximum > i32::MAX as u32
        || i.skill > i32::MAX as u32
        || i.formula_level > 8
        || !i.target_angle_degrees.is_finite()
        || i.life_damage.is_some_and(|v| !v.is_finite() || v < 0.0)
        || [
            i.rolls.variance,
            i.rolls.critical,
            f64::from(i.rolls.critical_defense),
            f64::from(i.rolls.sneak),
        ]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..1.0).contains(v))
    {
        return Err(E::InvalidState);
    }
    let a = i.source;
    let d = i.target;
    let pvp = a.player && d.player;
    let w = a.wand.as_ref();
    let skill = f64::from(i.skill);
    let mut base = i
        .life_damage
        .unwrap_or(f64::from(i.minimum) + f64::from(i.maximum - i.minimum) * i.rolls.variance);
    if i.life_damage.is_none()
        && let Some(w) = w
    {
        let mut elemental = if w.damage_type == i.damage_type {
            w.elemental_modifier
        } else {
            1.0
        };
        if pvp {
            elemental = ((elemental - 1.0) / 2.0) + 1.0;
        }
        base *= elemental;
    }
    let mut chance = w.filter(|w| w.biting != 0.0).map_or(0.05, |w| w.biting);
    let mut multiplier = 0.5 + w.map_or(0.0, |w| w.crushing);
    if let Some(w) = w
        && (i.school == MagicSchool::War
            || i.school == MagicSchool::Void && i.life_damage.is_none())
    {
        // Original DoCollision calls this before CalculateDamage sets isPvP.
        // The local isPvP in CalculateCriticalHitData is unused upstream, so its
        // configured PK imbue branch is unreachable on this projectile path.
        if w.imbues & 1 != 0 {
            chance = imbue(skill, 125., 360., chance, 0.5);
        }
        if w.imbues & 2 != 0 {
            multiplier = imbue(skill, 125., 360., multiplier, 5.0);
        }
    }
    let critical = i.rolls.critical < chance;
    let defended = i.projectile
        && critical
        && d.critical_defense
        && f64::from(i.rolls.critical_defense) < if a.player { 0.05 } else { 0.25 };
    let mut sneak = false;
    let mut sneak_rating = a.ratings.sneak;
    if i.projectile && a.sneak.advancement >= 2 {
        let mut rating = if a.sneak.advancement >= 3 { 20 } else { 10 };
        if a.sneak.current < i.skill {
            rating = (rating as f32 * (a.sneak.current as f32 / i.skill as f32)) as i32;
        }
        let angle = i.target_angle_degrees.rem_euclid(360.0);
        if (90.0..=270.0).contains(&angle) {
            sneak = true;
            sneak_rating = rating;
        } else if a.deception.advancement >= 2 {
            let mut chance = if a.deception.advancement >= 3 {
                0.15f32
            } else {
                0.1
            };
            if a.deception.current < 306 {
                chance *= (a.deception.current as f32 / 306.0).min(1.0);
            }
            if i.rolls.sneak < chance {
                if d.assess_person.advancement >= 2 {
                    rating = if d.assess_person.current < 306 {
                        (rating as f32 * (d.assess_person.current as f32 / 306.0).min(1.0)) as i32
                    } else {
                        0
                    };
                }
                if rating > 0 {
                    sneak_rating = rating;
                }
                sneak = true;
            } else {
                sneak_rating = 0;
            }
        }
    }
    let mut skill_bonus = 0.0;
    if a.player && i.school == MagicSchool::War {
        let mut minimum = i.minimum as f32;
        if pvp {
            minimum *= 0.5;
        }
        let difficulty = 100 + 50 * i.formula_level;
        // Pinned C++ unsigned/integer division is retained: below difficulty
        // the entire interval truncates to zero. Retail equivalence unproven.
        let mut modifier = if i.skill > difficulty {
            1.0f32
        } else if i.skill > difficulty - 75 {
            ((i.skill - (difficulty - 75)) / 75) as f32
        } else {
            0.0
        };
        if pvp {
            modifier *= 0.5;
        }
        if modifier > 0.0 {
            skill_bonus = f64::from(minimum * modifier);
        }
    }
    let mut slayer = 0.0;
    if i.projectile
        && let Some(w) = w
    {
        let cap = if w.slayer_type != 0 && w.slayer_type == d.creature_type {
            w.slayer_bonus
        } else {
            0.0
        };
        let m = imbue(skill, 125., 360., 1.0, cap);
        if m > 0.0 {
            slayer = base * (m - 1.0);
        }
    }
    let mut damage = base + skill_bonus + slayer;
    if critical {
        damage = base;
        if !defended {
            damage += if pvp {
                (damage / 2.0) * multiplier
            } else {
                damage * multiplier
            };
        }
        damage += skill_bonus;
        damage += slayer;
    }
    // Upstream labels this monster half-damage behavior unconfirmed.
    if !a.player {
        damage /= 2.0;
    }
    let ar = a.ratings;
    let dr = d.ratings;
    let mut rating = i64::from(ar.damage) - i64::from(ar.weakness) - i64::from(dr.resistance)
        + i64::from(dr.reckless);
    if sneak {
        rating += i64::from(sneak_rating);
    }
    if critical {
        rating += i64::from(ar.critical_damage) - i64::from(dr.critical_resistance);
    }
    if pvp {
        rating += i64::from(ar.pk_damage) - i64::from(dr.pk_resistance);
    }
    if d.player && d.magic_defense.advancement == 3 {
        rating -= i64::from((d.magic_defense.current as f32 / 50.0) as i32);
    }
    let mut rending = None;
    if let Some(w) = w {
        let flag = match i.damage_type {
            1 => 8,
            2 => 16,
            4 => 32,
            8 => 512,
            16 => 128,
            32 => 64,
            64 => 256,
            _ => 0,
        };
        if flag & w.imbues != 0 {
            rending = Some((0.25 + imbue(skill, 0., 360., 1., 2.25)).max(1.0));
        } else if w.resistance_cleaving == Some(i.damage_type) {
            rending = Some(2.25);
        }
    }
    if !damage.is_finite() {
        return Err(E::Overflow);
    }
    Ok(MagicDamageResult {
        before_rating: damage,
        rating: i32::try_from(rating).map_err(|_| E::Overflow)?,
        critical,
        critical_defended: defended,
        sneak,
        sneak_rating,
        rending,
        ignore_magic_resistance: a.ignore_magic_resistance
            || w.is_some_and(|w| w.ignore_magic_resistance),
    })
}
pub fn magic_resistance_index(damage_type: u32) -> Result<usize, E> {
    match damage_type {
        1 => Ok(0),
        2 => Ok(1),
        4 => Ok(2),
        8 => Ok(3),
        16 => Ok(4),
        32 => Ok(5),
        64 => Ok(6),
        1024 => Ok(7),
        128 => Ok(8),
        256 => Ok(9),
        512 => Ok(10),
        _ => Err(E::InvalidState),
    }
}
fn value(q: PhysicalQuality) -> f64 {
    (q.raw * q.increasing * q.decreasing + q.additive_increasing + q.additive_decreasing).max(0.0)
}
pub fn magic_damage_after_mitigation(
    i: &MagicDamageInput<'_>,
    r: MagicDamageResult,
    rating_modifier: f32,
) -> Result<u32, E> {
    if !rating_modifier.is_finite() || rating_modifier <= 0.0 || !r.before_rating.is_finite() {
        return Err(E::InvalidState);
    }
    let d = i.target;
    let pvp = i.source.player && d.player;
    let index = magic_resistance_index(i.damage_type)?;
    let mut q = d.resistances[index].quality;
    if let Some(rending) = r.rending
        && rending > q.increasing
    {
        q.increasing = rending;
    }
    let regular = if r.ignore_magic_resistance {
        q.raw as f32
    } else {
        value(q) as f32
    };
    let mut damage = r.before_rating * f64::from(rating_modifier);
    if !d.player || r.ignore_magic_resistance {
        if regular != 0.0 {
            damage *= f64::from(regular);
        }
    } else {
        let sum = d
            .base_strength
            .checked_add(d.base_endurance)
            .ok_or(E::Overflow)? as f32;
        let natural = (if sum <= 200.0 {
            1.0 - ((0.05 * f64::from(sum)) / 100.0)
        } else {
            1.0 - (((0.1666667 * f64::from(sum)) - 23.33333) / 100.0)
        } as f32)
            .max(0.5);
        if f64::from(natural) < (q.raw * q.decreasing + q.additive_decreasing).max(0.0) {
            q.decreasing = f64::from(natural);
            damage *= value(q);
        } else {
            damage *= f64::from(regular);
        }
    }
    if i.projectile {
        if d.missile_absorption {
            let mut reduction = (0.25 * f64::from(d.magic_defense.current) * 0.003) - 0.25 * 0.3;
            if reduction > 0.0 {
                reduction = reduction.min(0.25);
                if pvp {
                    reduction *= 0.72;
                }
                damage *= 1.0 - reduction;
            }
        }
        if i.target_in_combat
            && let Some((skill, cap)) = d.shield
            && cap > 0.0
            && skill.current >= 100
        {
            let st = if skill.advancement != 3 { 0.8_f32 } else { 1.0 };
            let reduction = ((f64::from(cap * st * skill.current as f32) * 0.003
                - f64::from(cap * st) * 0.3) as f32)
                .min(cap * st)
                .max(0.0);
            damage *= 1.0 - f64::from(reduction);
        }
    }
    if index < 8 && d.augmentation_family {
        damage *= 1.0 - 0.1 * f64::from(d.resistances[index].augmentation);
    }
    let rounded = (damage + f64::from(0.0002_f32)).floor();
    if !rounded.is_finite() || rounded > f64::from(i32::MAX) {
        return Err(E::Overflow);
    }
    Ok(rounded.max(0.0) as u32)
}
