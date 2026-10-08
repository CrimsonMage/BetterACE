//! Scalar pinned ACE specialization consumers. Callers supply authoritative
//! skill/geometry/equipment views and independent keyed RNG draws.
use bace_gameplay_api::SkillAdvancement;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CombatSkill {
    pub advancement: SkillAdvancement,
    pub base: u32,
    pub current: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefenseKind {
    Melee,
    Missile,
    Magic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecializationError {
    Nonfinite,
    InvalidRange,
    Overflow,
}
/// Pinned Creature_Rating.GetSpecDefenseBonus, for the corresponding attack type.
pub fn specialized_defense_rating(player: bool, kind: DefenseKind, skill: CombatSkill) -> u32 {
    if !player || skill.advancement != SkillAdvancement::Specialized {
        0
    } else {
        skill.base
            / match kind {
                DefenseKind::Melee => 60,
                DefenseKind::Missile | DefenseKind::Magic => 50,
            }
    }
}
/// Apply after shield item enchantments; physical armor formula remains caller-owned.
pub fn shield_armor_cap(skill: CombatSkill) -> u32 {
    if skill.advancement == SkillAdvancement::Specialized {
        skill.current
    } else {
        (skill.current as f32 / 2.0).round_ties_even() as u32
    }
}
/// Pinned Creature_Combat shield cap followed by SkillFormula.CalcArmorMod.
/// The prepared armor input already includes item modifiers and resistance.
pub fn shield_physical_modifier(
    skill: CombatSkill,
    angle_degrees: f32,
    in_combat: bool,
    effective_armor: f32,
) -> Result<f32, SpecializationError> {
    if !angle_degrees.is_finite() || !effective_armor.is_finite() {
        return Err(SpecializationError::Nonfinite);
    }
    if !in_combat || angle_degrees.abs() > 90.0 {
        return Ok(1.0);
    }
    let armor = effective_armor.min(shield_armor_cap(skill) as f32);
    let scale = 200.0f32 / 3.0;
    Ok(if armor > 0.0 {
        scale / (armor + scale)
    } else {
        1.0 - armor / scale
    })
}
/// Pinned Creature_Rating.GetNegativeRatingMod for nonnegative defense bonuses.
pub fn defense_rating_modifier(rating: u32) -> Result<f32, SpecializationError> {
    let denominator = rating
        .checked_add(100)
        .filter(|v| *v <= i32::MAX as u32)
        .ok_or(SpecializationError::Overflow)?;
    Ok(100.0 / denominator as f32)
}
/// Pinned SpellProjectile.GetShieldMod. Valid cap is the authoritative shield's
/// fractional absorption property. Geometry angle is accepted server geometry.
pub fn shield_magic_modifier(
    skill: CombatSkill,
    angle_degrees: f32,
    cap: f32,
) -> Result<f32, SpecializationError> {
    if !angle_degrees.is_finite() || !cap.is_finite() {
        return Err(SpecializationError::Nonfinite);
    }
    if !(0.0..=1.0).contains(&cap) {
        return Err(SpecializationError::InvalidRange);
    }
    if angle_degrees.abs() > 90.0
        || (skill.advancement as u32) < SkillAdvancement::Trained as u32
        || skill.base < 100
    {
        return Ok(1.0);
    }
    let base = skill.base.min(433) as f32;
    let spec = if skill.advancement == SkillAdvancement::Specialized {
        1.0
    } else {
        0.8
    };
    let reduction = (cap * spec * base * 0.003) - (cap * spec * 0.3);
    Ok(1.0f32.min(1.0 - reduction))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealingCheck {
    pub effective_skill: i32,
    pub difficulty: i32,
}
/// Healer.DoSkillCheck numeric inputs. Use the ordinary bounded skill contest
/// and a fresh keyed healing draw; this does not heal or consume a kit.
pub fn healing_check(
    skill: CombatSkill,
    kit_boost: i32,
    missing_vital: u32,
    in_combat: bool,
) -> Result<HealingCheck, SpecializationError> {
    let total = i64::from(skill.current) + i64::from(kit_boost);
    let effective = (total as f32
        * if skill.advancement == SkillAdvancement::Specialized {
            1.5
        } else {
            1.1
        })
    .round_ties_even();
    let difficulty = (missing_vital
        .checked_mul(2)
        .ok_or(SpecializationError::Overflow)? as f32
        * if in_combat { 1.1 } else { 1.0 })
    .round_ties_even();
    if f64::from(effective) > f64::from(i32::MAX)
        || f64::from(effective) < f64::from(i32::MIN)
        || f64::from(difficulty) > f64::from(i32::MAX)
    {
        return Err(SpecializationError::Overflow);
    }
    Ok(HealingCheck {
        effective_skill: effective as i32,
        difficulty: difficulty as i32,
    })
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SneakAttackInput {
    pub sneak: CombatSkill,
    pub deception: CombatSkill,
    pub attack_skill: u32,
    pub target_assess_person: u32,
    pub target_is_creature: bool,
    pub angle_degrees: f32,
    pub roll: f32,
}
/// Returns the multiplicative damage factor. Pinned behind test is |angle|>90.
pub fn sneak_attack_modifier(input: SneakAttackInput) -> Result<f32, SpecializationError> {
    validate_roll(input.roll)?;
    if !input.angle_degrees.is_finite() {
        return Err(SpecializationError::Nonfinite);
    }
    if !input.target_is_creature
        || (input.sneak.advancement as u32) < SkillAdvancement::Trained as u32
    {
        return Ok(1.0);
    }
    let behind = input.angle_degrees.abs() > 90.0;
    let mut chance = if behind {
        1.0
    } else {
        match input.deception.advancement {
            SkillAdvancement::Trained => 0.1,
            SkillAdvancement::Specialized => 0.15,
            _ => 0.0,
        }
    };
    if !behind && input.deception.current < 306 {
        chance *= (input.deception.current as f32 / 306.0).min(1.0);
    }
    if input.roll >= chance {
        return Ok(1.0);
    }
    let mut rating = if input.sneak.advancement == SkillAdvancement::Specialized {
        20.0
    } else {
        10.0
    };
    if input.sneak.current < input.attack_skill {
        rating *= input.sneak.current as f32 / input.attack_skill as f32;
    }
    if !behind {
        rating *= 1.0 - (input.target_assess_person as f32 / 306.0).min(1.0);
    }
    Ok((100.0 + rating) / 100.0)
}
/// Player_Combat.GetRecklessnessMod. The outer attack pipeline excludes critical
/// hits; the supplied bool makes that gate explicit here for safe consumption.
pub fn recklessness_modifier(
    skill: CombatSkill,
    attack_skill: u32,
    power: f32,
    melee_or_missile: bool,
    critical: bool,
) -> Result<f32, SpecializationError> {
    if !power.is_finite() {
        return Err(SpecializationError::Nonfinite);
    }
    if !(0.0..=1.0).contains(&power) {
        return Err(SpecializationError::InvalidRange);
    }
    if critical
        || !melee_or_missile
        || (skill.advancement as u32) < SkillAdvancement::Trained as u32
        || !(0.1..=0.9).contains(&power)
    {
        return Ok(1.0);
    }
    let mut rating = if skill.advancement == SkillAdvancement::Specialized {
        20
    } else {
        10
    };
    if skill.current < attack_skill {
        rating =
            (rating as f32 * (skill.current as f32 / attack_skill as f32)).round_ties_even() as u32;
    }
    Ok((100 + rating) as f32 / 100.0)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyAttackHeight {
    Low,
    Medium,
    High,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyFightingEffects {
    pub spells: [u32; 2],
    pub count: usize,
}
/// Spell IDs are the pinned SpellId enum; effect strengths/duration resolve from
/// prepared spell content. No direct damage or guessed registry entries are made.
pub fn dirty_fighting_effects(
    skill: CombatSkill,
    attack_skill: u32,
    height: DirtyAttackHeight,
    roll: f32,
) -> Result<DirtyFightingEffects, SpecializationError> {
    validate_roll(roll)?;
    let empty = DirtyFightingEffects {
        spells: [0; 2],
        count: 0,
    };
    if (skill.advancement as u32) < SkillAdvancement::Trained as u32 {
        return Ok(empty);
    }
    let mut chance = 0.25;
    if skill.current < attack_skill {
        chance *= skill.current as f32 / attack_skill as f32;
    }
    if roll >= chance {
        return Ok(empty);
    }
    let spec = skill.advancement == SkillAdvancement::Specialized;
    Ok(match height {
        DirtyAttackHeight::Low => DirtyFightingEffects {
            spells: [if spec { 5940 } else { 5944 }, 0],
            count: 1,
        },
        DirtyAttackHeight::Medium => DirtyFightingEffects {
            spells: [if spec { 5939 } else { 5943 }, 0],
            count: 1,
        },
        DirtyAttackHeight::High => DirtyFightingEffects {
            spells: if spec { [5938, 5941] } else { [5942, 5945] },
            count: 2,
        },
    })
}
fn validate_roll(roll: f32) -> Result<(), SpecializationError> {
    if !roll.is_finite() {
        Err(SpecializationError::Nonfinite)
    } else if !(0.0..1.0).contains(&roll) {
        Err(SpecializationError::InvalidRange)
    } else {
        Ok(())
    }
}
