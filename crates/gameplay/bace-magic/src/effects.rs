//! Typed spell effect families and source-backed vital calculations. All values
//! are prepared server data; client casts select IDs, never these parameters.
use bace_geometry::Vec3;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagicSchool {
    War = 1,
    Life = 2,
    Creature = 3,
    Item = 4,
    Void = 5,
}
impl MagicSchool {
    /// Native GDLE/ACE SchoolOfMagic IDs differ from BetterACE's frozen
    /// War/Life/Creature/Item/Void skill-array and recovery order.
    pub fn from_native_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Self::War),
            2 => Some(Self::Life),
            3 => Some(Self::Item),
            4 => Some(Self::Creature),
            5 => Some(Self::Void),
            _ => None,
        }
    }
    pub fn native_id(self) -> u32 {
        match self {
            Self::War => 1,
            Self::Life => 2,
            Self::Item => 3,
            Self::Creature => 4,
            Self::Void => 5,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vital {
    Health,
    Stamina,
    Mana,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileShape {
    Bolt,
    Streak,
    Arc,
    Ring,
    Volley,
    Blast,
    Wall,
    Strike,
    /// Unclassified authored GDLE grid/fan; physics still uses exact group data.
    Group,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentSpec {
    pub category: u16,
    pub power: u32,
    pub duration: f64,
    pub layer: u16,
    pub stat_type: u32,
    pub stat_key: u32,
    pub value: f32,
    pub beneficial: bool,
    pub set_id: Option<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectileSpec {
    pub template: u32,
    pub shape: ProjectileShape,
    pub count: u16,
    pub radius: f32,
    pub speed: f32,
    pub gravity: f32,
    pub tracking: bool,
    pub perturbation: Vec3,
    pub lifetime: f64,
    pub spread_degrees: f32,
    pub padding: Vec3,
    pub offset: Vec3,
    pub dimensions: [u16; 3],
    pub minimum_damage: u32,
    pub maximum_damage: u32,
    pub damage_type: u32,
    pub enchantment: Option<EnchantmentSpec>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DispelSpec {
    pub minimum_power: u32,
    pub maximum_power: u32,
    pub count: u32,
    pub harmful: bool,
    pub beneficial: bool,
    pub school: Option<MagicSchool>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PortalEffect {
    Link {
        slot: u32,
    },
    Recall {
        slot: u32,
    },
    Summon {
        slot: u32,
        template: u32,
        lifetime: f64,
    },
    Sending {
        cell: u32,
        position: Vec3,
        heading: f32,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum SpellEffect {
    Boost {
        vital: Vital,
        minimum: i32,
        maximum: i32,
    },
    Transfer {
        source: Vital,
        destination: Vital,
        source_is_caster: bool,
        destination_is_caster: bool,
        proportion: f32,
        loss: f32,
        cap: u32,
    },
    Enchantment(EnchantmentSpec),
    Projectile(ProjectileSpec),
    LifeProjectile {
        projectile: ProjectileSpec,
        source: Vital,
        proportion: f32,
        damage_ratio: f32,
    },
    Dispel(DispelSpec),
    Portal(PortalEffect),
    FellowshipBoost {
        vital: Vital,
        minimum: i32,
        maximum: i32,
    },
    FellowshipEnchantment(EnchantmentSpec),
    FellowshipDispel(DispelSpec),
    FellowshipPortal(PortalEffect),
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSpell {
    pub id: u32,
    pub school: MagicSchool,
    pub power: u32,
    pub base_mana: u32,
    pub range_constant: f32,
    pub range_per_skill: f32,
    pub harmful: bool,
    pub resistable: bool,
    pub effect: SpellEffect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VitalState {
    pub current: u32,
    pub maximum: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VitalChange {
    pub before: u32,
    pub after: u32,
    pub delta: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferChange {
    pub source: VitalChange,
    pub destination: VitalChange,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectError {
    InvalidState,
    InvalidRoll,
    InvalidModifier,
    Overflow,
    Capacity,
}
fn validate(state: VitalState) -> Result<(), EffectError> {
    if state.current > state.maximum || state.maximum > i32::MAX as u32 {
        Err(EffectError::InvalidState)
    } else {
        Ok(())
    }
}
/// WorldObject_Magic.HandleCastSpell_Boost: resistance is double precision and
/// Math.Round is ties-to-even. Caller supplies the inclusive source RNG result.
pub fn boost(
    state: VitalState,
    minimum: i32,
    maximum: i32,
    rolled: i32,
    resistance: f64,
) -> Result<VitalChange, EffectError> {
    validate(state)?;
    if rolled < minimum.min(maximum) || rolled > minimum.max(maximum) {
        return Err(EffectError::InvalidRoll);
    }
    if !resistance.is_finite() || resistance < 0.0 {
        return Err(EffectError::InvalidModifier);
    }
    let attempted = (f64::from(rolled) * resistance).round_ties_even();
    if attempted < f64::from(i32::MIN) || attempted > f64::from(i32::MAX) {
        return Err(EffectError::Overflow);
    }
    let after =
        (i64::from(state.current) + attempted as i64).clamp(0, i64::from(state.maximum)) as u32;
    Ok(VitalChange {
        before: state.current,
        after,
        delta: i64::from(after) - i64::from(state.current),
    })
}
/// ACE HandleCastSpell_Transfer, before optional cloak mitigation/procs. f32
/// multiplication boundaries and ties-to-even rounding retain the source order.
pub fn transfer(
    source: VitalState,
    destination: VitalState,
    proportion: f32,
    loss: f32,
    cap: u32,
    drain_modifier: f32,
    boost_modifier: f32,
) -> Result<TransferChange, EffectError> {
    validate(source)?;
    validate(destination)?;
    if !proportion.is_finite()
        || !(0.0..=1.0).contains(&proportion)
        || !loss.is_finite()
        || !(0.0..=1.0).contains(&loss)
        || [drain_modifier, boost_modifier]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(EffectError::InvalidModifier);
    }
    let round = |value: f32| -> Result<u32, EffectError> {
        let value = f64::from(value).round_ties_even();
        if value > f64::from(i32::MAX) || value < 0.0 {
            Err(EffectError::Overflow)
        } else {
            Ok(value as u32)
        }
    };
    let mut drain = round(source.current as f32 * proportion * drain_modifier)?;
    if cap != 0 {
        drain = drain.min(cap);
    }
    let mut gain = round(drain as f32 * (1.0 - loss) * boost_modifier)?;
    let missing = destination.maximum - destination.current;
    let max_gain = if cap == 0 { missing } else { missing.min(cap) };
    if gain > max_gain {
        drain = round(drain as f32 * (max_gain as f32 / gain as f32))?;
        gain = max_gain;
    }
    let actual = drain.min(source.current);
    Ok(TransferChange {
        source: VitalChange {
            before: source.current,
            after: source.current - actual,
            delta: -i64::from(actual),
        },
        destination: VitalChange {
            before: destination.current,
            after: destination.current + gain,
            delta: i64::from(gain),
        },
    })
}
