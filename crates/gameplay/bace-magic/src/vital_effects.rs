//! GDLE TransferVital/AdjustVital scalar ordering. Invalid overdraw and arithmetic
//! overflow are rejected atomically rather than importing unsigned underflow.
use crate::{EffectError as E, TransferChange, VitalChange, VitalState};
fn valid(v: VitalState) -> Result<(), E> {
    if v.current > v.maximum || v.maximum > i32::MAX as u32 {
        Err(E::InvalidState)
    } else {
        Ok(())
    }
}
fn integer(v: f64) -> Result<u32, E> {
    if !v.is_finite() || v < 0.0 || v > f64::from(i32::MAX) {
        Err(E::Overflow)
    } else {
        Ok(v as u32)
    }
}
pub fn gdle_transfer(
    source: VitalState,
    destination: VitalState,
    proportion: f32,
    loss: f32,
    cap: u32,
    drain: f64,
    boost: f64,
) -> Result<TransferChange, E> {
    valid(source)?;
    valid(destination)?;
    if !proportion.is_finite()
        || !(0.0..=1.0).contains(&proportion)
        || !loss.is_finite()
        || !(0.0..=1.0).contains(&loss)
        || [drain, boost].iter().any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(E::InvalidModifier);
    }
    let mut take = integer(f64::from(source.current as f32 * proportion) * drain)?;
    if cap != 0 && take > cap {
        take = cap;
    }
    let mut give = integer(f64::from(take) * (1.0 - f64::from(loss)) * boost)?;
    let room = destination.maximum - destination.current;
    if give > room {
        give = room;
        take = integer(f64::from(give) / (1.0 - f64::from(loss)))?;
    }
    if take > source.current {
        return Err(E::InvalidState);
    }
    Ok(TransferChange {
        source: VitalChange {
            before: source.current,
            after: source.current - take,
            delta: -i64::from(take),
        },
        destination: VitalChange {
            before: destination.current,
            after: destination.current + give,
            delta: i64::from(give),
        },
    })
}
pub fn gdle_heal(
    state: VitalState,
    amount: f64,
    rating_modifier: f32,
    health: bool,
) -> Result<VitalChange, E> {
    valid(state)?;
    if !rating_modifier.is_finite() || rating_modifier <= 0.0 {
        return Err(E::InvalidModifier);
    }
    let mut amount = integer(amount)?;
    if health {
        amount = integer(f64::from(amount as f32 * rating_modifier))?;
    }
    let after = state.current.saturating_add(amount).min(state.maximum);
    Ok(VitalChange {
        before: state.current,
        after,
        delta: i64::from(after - state.current),
    })
}
pub fn gdle_natural_resistance(strength: u32, endurance: u32) -> Result<f32, E> {
    let sum = strength.checked_add(endurance).ok_or(E::Overflow)? as f32;
    Ok(((if sum <= 200.0 {
        1.0 - 0.05 * f64::from(sum) / 100.0
    } else {
        1.0 - (0.1666667 * f64::from(sum) - 23.33333) / 100.0
    }) as f32)
        .max(0.5))
}
