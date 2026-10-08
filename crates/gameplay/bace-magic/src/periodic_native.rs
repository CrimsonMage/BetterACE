//! GDLE CheckForTickingDots/CheckForTickingHots aggregate selection. ShowDotSpells
//! uses the pinned default false: it does not invent Aetheria's missing source.
use crate::{EffectError as E, EnchantmentRegistry, enchant_physical_quality};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePeriodicKind {
    Base,
    Nether,
    Healing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativePeriodicEffect {
    pub source: Option<u32>,
    pub amount: u32,
    pub kind: NativePeriodicKind,
}
pub fn gdle_periodic_batches(
    registry: &EnchantmentRegistry,
    flags: impl Fn(u32) -> Option<u32>,
) -> Result<Vec<NativePeriodicEffect>, E> {
    let mut df = None;
    let mut nether = None;
    let mut heals = 0usize;
    for entry in registry.entries() {
        if flags(entry.spell).is_some_and(|f| f & 0x10000 != 0) {
            match entry.spec.category {
                685 => df = Some(entry.caster),
                636..=638 => nether = Some(entry.caster),
                _ => {}
            }
        }
        if matches!(entry.spec.category, 617 | 630) {
            heals += 1;
        }
    }
    let mut out = Vec::with_capacity(2 + heals);
    for (key, kind, source, repetitions) in [
        (318, NativePeriodicKind::Base, df, 1),
        (330, NativePeriodicKind::Nether, nether, 1),
        (312, NativePeriodicKind::Healing, None, heals),
    ] {
        if repetitions == 0 || crate::enchantment_modifiers(registry, 4, key).is_empty() {
            continue;
        }
        let amount = enchant_physical_quality(registry, 4, key, 0., true)?.1 as u32;
        for _ in 0..repetitions {
            out.push(NativePeriodicEffect {
                source: source.filter(|id| *id != 0),
                amount,
                kind,
            });
        }
    }
    Ok(out)
}
pub fn gdle_periodic_damage(
    amount: u32,
    nether: bool,
    augmentation: u32,
    dot_resistance: u32,
    player: bool,
    player_modifier: f64,
) -> Result<u32, E> {
    if !player_modifier.is_finite() || !(0.0..=1000.0).contains(&player_modifier) {
        return Err(E::InvalidModifier);
    }
    let mut damage = f64::from(amount);
    if nether {
        let rating = augmentation
            .checked_add(dot_resistance)
            .ok_or(E::Overflow)?;
        damage *= 100.0 / (100.0 + f64::from(rating));
        if player {
            damage *= player_modifier;
        }
    }
    let rounded = (damage + f64::from(0.0002f32)).floor();
    if !rounded.is_finite() || rounded > f64::from(i32::MAX) {
        return Err(E::Overflow);
    }
    Ok(rounded as u32)
}
