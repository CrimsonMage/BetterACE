use super::maneuver::skill;
use super::{PhysicalError, SelectedPhysicalAttack};
use bace_gameplay_api::weapon_combat::*;
#[derive(Clone, Copy)]
pub struct PhysicalDamageInput<'a> {
    pub pk_override: Option<(PkStatus, PkStatus)>,
    pub rating_override: Option<(PhysicalRatings, PhysicalRatings)>,
    pub attacker: &'a PhysicalCombatProfile,
    pub defender: &'a PhysicalCombatProfile,
    pub selected: SelectedPhysicalAttack,
    pub kind: PhysicalKind,
    pub hook_part: u32,
    pub height: u32,
    pub original_power: f32,
    pub power: f32,
    pub target_angle_degrees: f64,
    pub defender_stamina: u32,
    pub defender_in_combat: bool,
    pub rolls: PhysicalRolls,
}
/// Verbatim scalar ordering from CombatFormulas::GetImbueMultiplier.
pub fn imbue(skill: f64, min: f64, max: f64, base: f64, cap: f64) -> f64 {
    if skill < min {
        return base;
    }
    if skill > max {
        return cap;
    }
    (((skill - min) / (max - min)) * cap).max(0.0).min(cap)
}
/// Frozen evasion, critical decision and weapon base at the source contact point.
/// Proc effects may change mitigation afterward, but cannot reroll this contact.
#[derive(Clone, Copy, Debug)]
pub struct PhysicalContact {
    initial: PhysicalImpact,
    base: f64,
    index: usize,
    imbues: u32,
    minimum: f64,
    maximum: f64,
    critical_multiplier: f64,
}
impl PhysicalContact {
    pub fn evaded(self) -> bool {
        self.initial.evaded
    }
    pub fn weapon_proc(self) -> Option<u32> {
        self.initial.weapon_proc
    }
}
pub fn resolve_physical(i: PhysicalDamageInput<'_>) -> Result<PhysicalImpact, PhysicalError> {
    finish_physical_contact(i, prepare_physical_contact(i)?)
}
pub fn prepare_physical_contact(
    i: PhysicalDamageInput<'_>,
) -> Result<PhysicalContact, PhysicalError> {
    let a = i.attacker;
    let d = i.defender;
    let (ar, _) = i.rating_override.unwrap_or((a.ratings, d.ratings));
    let r = i.rolls;
    let (ap, bp) = i.pk_override.unwrap_or((a.pk, d.pk));
    if !super::physical_permission_with_status(a, d, ap, bp) {
        return Err(PhysicalError::Forbidden);
    }
    if !(1..=3).contains(&i.height)
        || !i.original_power.is_finite()
        || !(0.0..=1.0).contains(&i.original_power)
        || !i.power.is_finite()
        || !(0.0..=1.0).contains(&i.power)
        || !i.target_angle_degrees.is_finite()
        || [
            r.evade,
            r.critical,
            r.dirty,
            r.weapon_proc,
            f64::from(r.variance),
            f64::from(r.body_part),
            f64::from(r.critical_defense),
            f64::from(r.sneak),
        ]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..1.0).contains(v))
    {
        return Err(PhysicalError::InvalidInput);
    }
    let w = if i.kind == PhysicalKind::Missile {
        a.launcher.as_ref()
    } else {
        match i.selected.hand {
            PhysicalHand::Main => a.main.as_ref(),
            PhysicalHand::Offhand => a.offhand.as_ref(),
            PhysicalHand::Unarmed => None,
        }
    };
    let mut out = PhysicalImpact {
        damage: 0,
        damage_type: 0,
        body_part: 0,
        critical: false,
        critical_defended: false,
        evaded: false,
        attack_conditions: 0,
        attacker_reckless_rating: ar.reckless,
        attacker_sneak_rating: ar.sneak,
        dirty_spells: [0; 2],
        dirty_count: 0,
        weapon_proc: None,
    };
    let defense_id = if i.kind == PhysicalKind::Melee { 6 } else { 7 };
    let defense = skill(d, defense_id);
    let modifier = if i.kind == PhysicalKind::Melee {
        d.melee_defense_modifier
    } else {
        d.missile_defense_modifier
    };
    let defense_level = (f64::from(defense.current) * modifier).round();
    if defense_level > f64::from(i32::MAX) {
        return Err(PhysicalError::Overflow);
    }
    if i.defender_stamina > 0 && defense.current > 0 {
        let chance = 1.0
            - (1.0 / (1.0 + (0.03 * (f64::from(i.selected.skill_level) - defense_level)).exp()));
        if r.evade > chance.clamp(0.0, 1.0) {
            out.evaded = true;
            return Ok(PhysicalContact {
                initial: out,
                base: 0.0,
                index: 0,
                imbues: 0,
                minimum: 0.0,
                maximum: 0.0,
                critical_multiplier: 0.0,
            });
        }
    }
    let (mut damage, mut variance, mut dtype) = if i.kind == PhysicalKind::Missile {
        let projectile = a
            .ammunition
            .as_ref()
            .or(w)
            .ok_or(PhysicalError::MissingWeapon)?;
        (
            projectile.damage
                * a.missile
                    .ok_or(PhysicalError::InvalidInput)?
                    .damage_modifier,
            projectile.variance,
            if projectile.damage_type == 0 {
                w.map_or(2, |w| if w.damage_type == 0 { 2 } else { w.damage_type })
            } else {
                projectile.damage_type
            },
        )
    } else if let Some(w) = w {
        (w.damage, w.variance, w.damage_type)
    } else {
        let body = a
            .body_attacks
            .iter()
            .find(|v| v.part == i.hook_part)
            .ok_or(PhysicalError::MissingBody)?;
        (body.damage, body.variance, body.damage_type)
    };
    if i.selected.hand == PhysicalHand::Unarmed
        && i.kind == PhysicalKind::Melee
        && let Some(item) = if i.original_power >= 0.75 {
            &a.boots
        } else {
            &a.gloves
        }
    {
        damage += item.damage;
        variance = item.variance;
        dtype = item.damage_type;
    }
    if dtype == 3 {
        let motion = a
            .maneuvers
            .get(i.selected.maneuver)
            .ok_or(PhysicalError::MissingMotion)?
            .motion;
        dtype = if i.original_power >= 0.25 && !(0x10000125..=0x1000012a).contains(&motion) {
            1
        } else {
            2
        };
    }
    if dtype == 17 {
        dtype = if i.original_power >= 0.25 { 1 } else { 16 };
    }
    let di = match dtype {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        16 => 4,
        32 => 5,
        64 => 6,
        1024 => 7,
        _ => return Err(PhysicalError::InvalidInput),
    };
    out.damage_type = dtype;
    let minimum = if i.kind == PhysicalKind::Melee {
        150.0
    } else {
        125.0
    };
    let maximum = if i.kind == PhysicalKind::Melee {
        400.0
    } else {
        360.0
    };
    let level = f64::from(i.selected.skill_level);
    let imbues = w.map_or(0, |w| w.imbues);
    let mut chance = w.filter(|w| w.biting != 0.0).map_or(0.1, |w| w.biting);
    let mut critical_multiplier = 1.0 + w.map_or(0.0, |w| w.crushing);
    if imbues & 1 != 0 {
        chance = imbue(level, minimum, maximum, chance, 0.5);
    }
    if imbues & 2 != 0 {
        critical_multiplier = imbue(level, minimum, maximum, critical_multiplier, 7.0);
    }
    out.critical = r.critical < chance;
    let base = damage
        * if out.critical {
            1.0
        } else {
            f64::from(1.0 - r.variance * variance)
        }
        * if i.kind == PhysicalKind::Melee {
            0.5 + f64::from(i.power)
        } else {
            1.0
        };
    out.weapon_proc = w
        .filter(|w| r.weapon_proc < w.proc_chance)
        .and_then(|w| w.proc_spell);
    Ok(PhysicalContact {
        initial: out,
        base,
        index: di,
        imbues,
        minimum,
        maximum,
        critical_multiplier,
    })
}
/// Resolve post-proc target-dependent mitigation from a retained contact.
pub fn finish_physical_contact(
    i: PhysicalDamageInput<'_>,
    contact: PhysicalContact,
) -> Result<PhysicalImpact, PhysicalError> {
    let PhysicalContact {
        initial: mut out,
        base,
        index: di,
        imbues,
        minimum,
        maximum,
        critical_multiplier,
    } = contact;
    if out.evaded {
        return Ok(out);
    }
    let a = i.attacker;
    let d = i.defender;
    let (ar, dr) = i.rating_override.unwrap_or((a.ratings, d.ratings));
    let r = i.rolls;
    let angle = i.target_angle_degrees.rem_euclid(360.0);
    let w = if i.kind == PhysicalKind::Missile {
        a.launcher.as_ref()
    } else {
        match i.selected.hand {
            PhysicalHand::Main => a.main.as_ref(),
            PhysicalHand::Offhand => a.offhand.as_ref(),
            PhysicalHand::Unarmed => None,
        }
    };
    let defense = skill(d, if i.kind == PhysicalKind::Melee { 6 } else { 7 });
    let level = f64::from(i.selected.skill_level);
    let dtype = out.damage_type;
    let mut damage;
    let hollow = w.is_some_and(|w| w.ignore_magic_armor || w.ignore_magic_resistance);
    let coordination = i.selected.skill == 46
        || i.selected.skill == 47 && w.is_none_or(|w| w.style & (0x80 | 0x400 | 0x800) == 0)
        || i.selected.skill == 49 && w.is_some_and(|w| w.skill == 46);
    let attribute = if coordination {
        a.coordination
    } else {
        a.strength
    };
    let attribute_mod = if attribute >= 1_000_000 {
        (f64::from(attribute) - 55.0) / 33.0
    } else {
        6.75 * (1.0 - (-0.005 * (f64::from(attribute) - 55.0)).exp())
    };
    let bonus = base
        * if attribute_mod < 0.0 || hollow {
            attribute_mod / 2.0
        } else {
            attribute_mod - 1.0
        };
    let slayer = w
        .filter(|w| w.slayer_type != 0 && w.slayer_type == d.creature_type)
        .map_or(0.0, |w| w.slayer_bonus);
    let sm = imbue(level, minimum, maximum, 1.0, slayer);
    damage = base + bonus + if sm > 0.0 { base * (sm - 1.0) } else { 0.0 };
    out.critical_defended = out.critical
        && d.critical_defense
        && r.critical_defense < if a.player { 0.05 } else { 0.25 };
    if out.critical_defended {
        out.attack_conditions |= 1;
    }
    if out.critical && !out.critical_defended {
        damage += damage * critical_multiplier;
    }
    let reckless = skill(a, 50);
    if out.critical {
        out.attacker_reckless_rating = 0;
    } else if reckless.advancement >= 2 && i.power > 0.1 && i.power < 0.9 {
        let rating = if reckless.advancement >= 3 { 20 } else { 10 };
        out.attacker_reckless_rating = if reckless.current < i.selected.skill_level {
            (rating as f32 * (reckless.current as f32 / i.selected.skill_level as f32)) as i32
        } else {
            rating
        };
        out.attack_conditions |= 2;
    } else {
        out.attacker_reckless_rating = 0;
    }
    let sneak = skill(a, 51);
    let mut sneak_applied = false;
    if sneak.advancement >= 2 {
        let mut rating = if sneak.advancement >= 3 { 20 } else { 10 };
        if sneak.current < i.selected.skill_level {
            rating =
                (rating as f32 * (sneak.current as f32 / i.selected.skill_level as f32)) as i32;
        }
        if (90.0..=270.0).contains(&angle) {
            out.attacker_sneak_rating = rating;
            sneak_applied = true;
        } else {
            let deception = skill(a, 20);
            if deception.advancement >= 2 {
                let chance = if deception.advancement >= 3 {
                    0.15f32
                } else {
                    0.1f32
                } * (deception.current as f32 / 306.0).min(1.0);
                if r.sneak < chance {
                    let assess = skill(d, 19);
                    if assess.advancement >= 2 {
                        rating = if assess.current < 306 {
                            (rating as f32 * (assess.current as f32 / 306.0).min(1.0)) as i32
                        } else {
                            0
                        };
                    }
                    if rating > 0 {
                        out.attacker_sneak_rating = rating;
                    }
                    sneak_applied = true;
                } else {
                    out.attacker_sneak_rating = 0;
                }
            }
        }
    }
    if sneak_applied {
        out.attack_conditions |= 4;
    }
    let mut rating = i64::from(ar.damage) - i64::from(ar.weakness) - i64::from(dr.resistance)
        + i64::from(dr.reckless);
    if out.attack_conditions & 2 != 0 {
        rating += i64::from(out.attacker_reckless_rating);
    }
    if sneak_applied {
        rating += i64::from(out.attacker_sneak_rating);
    }
    if out.critical {
        rating += i64::from(ar.critical_damage) - i64::from(dr.critical_resistance);
    }
    if a.player && d.player {
        rating += i64::from(ar.pk_damage) - i64::from(dr.pk_resistance);
    }
    if d.player && defense.advancement == 3 {
        rating -= i64::from(
            defense.current
                / if i.kind == PhysicalKind::Melee {
                    60
                } else {
                    50
                },
        );
    }
    let rating = i32::try_from(rating).map_err(|_| PhysicalError::Overflow)?;
    // Explicit GDLE defect correction: negative ratings use ACE inverse rating.
    // GDLE additive rating composition and positive branch remain unchanged.
    let rating_mod = physical_rating_modifier(rating)?;
    damage *= f64::from(rating_mod);
    let quadrant = (3 - i.height) as usize * 4
        + if angle < 180.0 { 2 } else { 0 }
        + usize::from((90.0..270.0).contains(&angle));
    let mut sum = 0.0f32;
    let mut body = None;
    for part in &d.armor {
        let weight = part.hit_weights[quadrant];
        if weight <= 0.0 {
            continue;
        }
        if r.body_part <= weight + sum {
            body = Some(part);
            break;
        }
        sum += weight;
    }
    let body = body
        .or_else(|| d.armor.iter().find(|p| p.part == 0))
        .ok_or(PhysicalError::MissingBody)?;
    out.body_part = body.part;
    let mut resistance = d.resistances[di];
    let rend_flag = [8, 16, 32, 128, 512, 64, 256, 0][di];
    if imbues & rend_flag != 0 || w.is_some_and(|w| w.resistance_cleaving == Some(dtype)) {
        let rend =
            if w.is_some_and(|w| w.resistance_cleaving == Some(dtype)) && imbues & rend_flag == 0 {
                if i.kind == PhysicalKind::Melee {
                    2.5
                } else {
                    2.25
                }
            } else if i.kind == PhysicalKind::Melee {
                imbue(level, 0.0, 400.0, 1.0, 2.5).max(1.0)
            } else {
                (0.25 + imbue(level, 0.0, 360.0, 1.0, 2.25)).max(1.0)
            };
        resistance.quality.increasing = resistance.quality.increasing.max(rend);
    }
    let ignore_resist = w.is_some_and(|w| w.ignore_magic_resistance);
    let regular = if ignore_resist {
        resistance.quality.raw
    } else {
        super::armor::enchanted(resistance.quality)
    } as f32;
    if d.player && !ignore_resist {
        let attrs = d
            .base_strength
            .checked_add(d.base_endurance)
            .ok_or(PhysicalError::Overflow)? as f32;
        let natural = (if attrs <= 200.0 {
            (1.0 - ((0.05 * f64::from(attrs)) / 100.0)) as f32
        } else {
            (1.0 - (((0.1666667 * f64::from(attrs)) - 23.33333) / 100.0)) as f32
        })
        .max(0.5);
        if f64::from(natural) < super::armor::decreasing_only(resistance.quality) {
            resistance.quality.decreasing = f64::from(natural);
            damage *= super::armor::enchanted(resistance.quality);
        } else {
            damage *= f64::from(regular);
        }
    } else if regular != 0.0 {
        damage *= f64::from(regular);
    }
    if imbues & 0x80000000 == 0 {
        let rending = if imbues & 4 != 0 {
            Some(if i.kind == PhysicalKind::Melee {
                1.0 / imbue(level, 150.0, 400.0, 1.0, 2.5).max(1.0)
            } else {
                1.0 / (0.25 + imbue(level, 125.0, 360.0, 1.0, 2.25)).max(1.0)
            })
        } else {
            None
        };
        // GDLE computes armor-cleaving metadata but only isArmorRending gates
        // the Monster armor consumer. Preserve that pinned behavior explicitly.
        let armor = super::armor::effective_armor(super::armor::ArmorInput {
            defender: d,
            body,
            index: di,
            raw: w.is_some_and(|w| w.ignore_magic_armor),
            front: !(90.0..270.0).contains(&angle),
            ignore_shield: a.ignore_shield,
            rending,
        })?;
        let factor = if armor >= 0.0 {
            1.0 / (1.0 + f64::from(armor) / (190.0 / 3.0))
        } else {
            1.0 + f64::from(-armor) / (190.0 / 3.0)
        } as f32;
        damage *= f64::from(factor);
    }
    damage *= 1.0 - 0.1 * f64::from(resistance.augmentation);
    if !damage.is_finite() || damage < 0.0 || damage > f64::from(u32::MAX) {
        return Err(PhysicalError::Overflow);
    }
    out.damage = (damage - f64::from(0.0002f32)).ceil().max(0.0) as u32;
    (out.dirty_spells, out.dirty_count) =
        physical_dirty_spells(a, i.selected.skill_level, i.height, r.dirty)?;
    Ok(out)
}

pub fn physical_dirty_spells(
    attacker: &PhysicalCombatProfile,
    attack_skill: u32,
    height: u32,
    roll: f64,
) -> Result<([u32; 2], usize), PhysicalError> {
    if !(1..=3).contains(&height) || !roll.is_finite() || !(0.0..1.0).contains(&roll) {
        return Err(PhysicalError::InvalidInput);
    }
    let level = f64::from(attack_skill);
    let mut spells = [0; 2];
    let mut count = 0;
    if attacker.player {
        let dirty = skill(attacker, 52);
        let chance = 0.25 * (f64::from(dirty.current) / level).min(1.0);
        if dirty.current > 0 && dirty.advancement >= 2 && roll < chance {
            let spec = dirty.advancement == 3;
            spells = dirty_spells_for_height(spec, height);
            count = if height == 1 { 2 } else { 1 };
        }
    }
    Ok((spells, count))
}

pub fn physical_rating_modifier(rating: i32) -> Result<f32, PhysicalError> {
    let m = if rating > 0 {
        (100.0 + rating as f32) / 100.0
    } else if rating < 0 {
        100.0 / (100.0 - rating as f32)
    } else {
        1.0
    };
    if m.is_finite() && m > 0.0 {
        Ok(m)
    } else {
        Err(PhysicalError::InvalidInput)
    }
}

fn dirty_spells_for_height(spec: bool, height: u32) -> [u32; 2] {
    match height {
        1 => {
            if spec {
                [5938, 5941]
            } else {
                [5942, 5945]
            }
        }
        2 => [if spec { 5939 } else { 5943 }, 0],
        _ => [if spec { 5940 } else { 5944 }, 0],
    }
}
/// Cold dependency closure includes both training tiers because a live player
/// can train/specialize without changing equipment. NPCs do not use this proc.
pub fn physical_dirty_spell_dependencies(profile: &PhysicalCombatProfile) -> Vec<u32> {
    if !profile.player {
        return Vec::new();
    }
    let mut spells = Vec::with_capacity(8);
    for specialized in [false, true] {
        for height in 1..=3 {
            spells.extend(
                dirty_spells_for_height(specialized, height)
                    .into_iter()
                    .filter(|id| *id != 0),
            );
        }
    }
    spells.sort_unstable();
    spells.dedup();
    spells
}
