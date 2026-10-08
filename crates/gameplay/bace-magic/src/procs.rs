//! GDLE cloak/Aetheria decision policy. Outputs are requests, never completed
//! casts. Current target, item ownership and spell execution belong to owners.
use crate::EffectError as E;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagicCloakEffect {
    Absorb,
    Spell(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MagicCloak {
    pub item: u32,
    pub level: u32,
    pub effect: MagicCloakEffect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MagicSigil {
    pub item: u32,
    pub slot: u8,
    pub level: u32,
    pub spell: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MagicProcPolicy {
    pub cloak_base: f32,
    pub cloak_half: f32,
    pub cloak_quarter: f32,
    pub cloak_tenth: f32,
    pub cloak_level: f32,
    pub sigil_rates: [f32; 3],
}
impl Default for MagicProcPolicy {
    fn default() -> Self {
        Self {
            cloak_base: 0.05,
            cloak_half: 0.075,
            cloak_quarter: 0.05,
            cloak_tenth: 0.025,
            cloak_level: 0.01,
            sigil_rates: [0.005, 0.0075, 0.01],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MagicItemProc {
    pub item: u32,
    pub target: u32,
    pub spell: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MagicCloakResult {
    pub damage: u32,
    pub cast: Option<MagicItemProc>,
}
/// Exact C++ unsigned integer division is retained for the health thresholds.
/// The misleading C++ `maxHealth` local actually queries CURRENT health (key2).
/// Integer division only earns the half-health tier at damage >= current health.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MagicCloakInput {
    pub cloak: MagicCloak,
    pub damage: u32,
    pub current_health: u32,
    pub player_vs_player: bool,
    pub owner: u32,
    pub current_enemy: Option<u32>,
    pub roll: f64,
    pub policy: MagicProcPolicy,
}
pub fn magic_cloak_proc(input: MagicCloakInput) -> Result<MagicCloakResult, E> {
    let MagicCloakInput {
        cloak,
        damage,
        current_health,
        player_vs_player,
        owner,
        current_enemy,
        roll,
        policy,
    } = input;
    if current_health == 0
        || owner == 0
        || cloak.item == 0
        || cloak.level > 100
        || !roll.is_finite()
        || !(0.0..1.0).contains(&roll)
    {
        return Err(E::InvalidState);
    }
    let fraction = f64::from(damage / current_health);
    let mut chance = f64::from(policy.cloak_base);
    chance += if fraction >= 0.5 {
        f64::from(policy.cloak_half)
    } else if fraction >= 0.25 {
        f64::from(policy.cloak_quarter)
    } else if fraction >= 0.1 {
        f64::from(policy.cloak_tenth)
    } else {
        0.0
    };
    if cloak.level > 1 {
        chance += f64::from((cloak.level - 1) as f32 * policy.cloak_level);
    }
    if !chance.is_finite() || chance < 0.0 {
        return Err(E::InvalidState);
    }
    let mut result = MagicCloakResult { damage, cast: None };
    if roll <= chance {
        match cloak.effect {
            MagicCloakEffect::Absorb => {
                result.damage = damage.saturating_sub(if player_vs_player { 100 } else { 200 })
            }
            MagicCloakEffect::Spell(spell) => {
                let target = if (5754..=5756).contains(&spell) {
                    current_enemy
                } else {
                    Some(owner)
                };
                if let Some(target) = target
                    && target != 0
                    && spell != 0
                {
                    result.cast = Some(MagicItemProc {
                        item: cloak.item,
                        target,
                        spell,
                    });
                }
            }
        }
    }
    Ok(result)
}
/// Original ordered spell-rate map, duplicate-slot bonuses, and last-slot item
/// caster. The pinned self-target bit test has C++ precedence `flags & (8==1)`;
/// it is false, so only spell5208 or a resisted/missed target selects the owner.
pub fn magic_sigil_procs(
    sigils: &[MagicSigil],
    owner: u32,
    target: Option<u32>,
    draws: &[f64],
    policy: MagicProcPolicy,
    flags: impl Fn(u32) -> Option<u32>,
) -> Result<Vec<MagicItemProc>, E> {
    if sigils.len() > 3 || owner == 0 {
        return Err(E::InvalidState);
    }
    let mut slots = [None; 3];
    for sigil in sigils {
        let index = usize::from(sigil.slot);
        if index >= 3
            || slots[index].is_some()
            || sigil.item == 0
            || sigil.spell == 0
            || sigil.level > 100
        {
            return Err(E::InvalidState);
        }
        slots[index] = Some(*sigil);
    }
    let mut rates: Vec<(u32, f64)> = Vec::with_capacity(3);
    let mut caster = 0;
    for (slot, sigil) in slots.into_iter().enumerate() {
        if let Some(sigil) = sigil {
            caster = sigil.item;
            let rate = f64::from(policy.sigil_rates[slot] * sigil.level as f32);
            if !rate.is_finite() || rate < 0.0 {
                return Err(E::InvalidState);
            }
            if let Some(old) = rates.iter_mut().find(|p| p.0 == sigil.spell) {
                old.1 += rate + if slot == 1 { 0.0025 } else { 0.005 };
            } else {
                rates.push((sigil.spell, rate));
            }
        }
    }
    rates.sort_by_key(|p| p.0);
    if draws.len() < rates.len()
        || draws[..rates.len()]
            .iter()
            .any(|v| !v.is_finite() || !(0.0..1.0).contains(v))
    {
        return Err(E::InvalidRoll);
    }
    let mut out = Vec::with_capacity(rates.len());
    for ((spell, rate), roll) in rates.into_iter().zip(draws) {
        if *roll > rate {
            continue;
        }
        let Some(flags) = flags(spell) else {
            continue;
        };
        if flags & 8 == 0 && target.is_none() {
            continue;
        }
        out.push(MagicItemProc {
            item: caster,
            target: if spell == 5208 {
                owner
            } else {
                target.unwrap_or(owner)
            },
            spell,
        });
    }
    Ok(out)
}
