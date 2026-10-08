//! GDLE EnchantedQualityDetails, clothing coverage and layered armor ordering.
use super::PhysicalError;
use bace_gameplay_api::weapon_combat::*;
pub fn enchanted(q: PhysicalQuality) -> f64 {
    let mut v = q.raw;
    v *= q.increasing;
    v *= q.decreasing;
    v += q.additive_increasing;
    v += q.additive_decreasing;
    if v.is_finite() { v.max(0.0) } else { v }
}
pub fn decreasing_only(q: PhysicalQuality) -> f64 {
    let v = q.raw * q.decreasing + q.additive_decreasing;
    if v.is_finite() { v.max(0.0) } else { v }
}
pub fn clothing_coverage(part: u32, coverage: u32) -> f32 {
    let (armor, wear) = match part {
        0 => (1, 0),
        1 => (0x200, 2),
        2 => (0x400, 4),
        3 => (0x800, 8),
        4 => (0x1000, 0x10),
        5 => (0x20, 0),
        6 => (0x2000, 0x40),
        7 => (0x4000, 0x80),
        8 => (0x100, 0),
        _ => (0, 0),
    };
    if coverage & armor != 0 {
        1.0
    } else if coverage & wear != 0 {
        0.5
    } else {
        0.0
    }
}
pub(super) struct ArmorInput<'a> {
    pub defender: &'a PhysicalCombatProfile,
    pub body: &'a PhysicalBodyDefense,
    pub index: usize,
    pub raw: bool,
    pub front: bool,
    pub ignore_shield: f64,
    pub rending: Option<f64>,
}
pub(super) fn effective_armor(input: ArmorInput<'_>) -> Result<f32, PhysicalError> {
    let d = input.defender;
    let index = input.index;
    let mut quality = input.body.armor[index];
    for layer in d.armor_layers.iter().chain(d.shield.iter()) {
        if layer.shield && !input.front {
            continue;
        }
        let factor = if layer.clothing && !layer.shield {
            clothing_coverage(input.body.part, layer.coverage)
        } else {
            1.0
        };
        if factor == 0.0 {
            continue;
        }
        let mut armor = if input.raw {
            layer.armor.raw
        } else {
            enchanted(layer.armor)
        } as f32;
        if layer.shield {
            let cap = if d.shield_skill.advancement == 3 {
                d.shield_skill.current
            } else {
                d.shield_skill.current / 2
            };
            armor = armor.min(cap as f32);
            armor = (f64::from(armor) * (1.0 - input.ignore_shield)) as f32;
        }
        let modifier = if input.raw {
            layer.modifiers[index].raw
        } else {
            enchanted(layer.modifiers[index])
        }
        .clamp(0.0, 2.0);
        armor = (f64::from(armor) * modifier) as f32;
        quality.raw += f64::from(armor * factor);
    }
    if let Some(rending) = input.rending
        && rending < quality.decreasing
    {
        quality.decreasing = rending;
    }
    let armor = if input.raw {
        quality.raw
    } else {
        enchanted(quality)
    } as f32;
    if armor.is_finite() {
        Ok(armor)
    } else {
        Err(PhysicalError::Overflow)
    }
}
pub(super) fn valid_quality(q: PhysicalQuality) -> bool {
    [
        q.raw,
        q.increasing,
        q.decreasing,
        q.additive_increasing,
        q.additive_decreasing,
    ]
    .into_iter()
    .all(f64::is_finite)
        && q.increasing >= 0.0
        && q.decreasing >= 0.0
}
