//! Active ACE RecipeManager.TryMutate -> embedded Effect/EffectArgument semantics.
//! Missing numeric destination qualities resolve to typed zero for arithmetic;
//! missing numeric source qualities skip that effect, as Effect.Validate does.
use crate::{CraftError, PropertyKey, PropertyKind as K, PropertyValue as V};
use std::collections::BTreeMap;

pub(crate) enum Op {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    AtLeastAdd,
}
pub(crate) enum Argument {
    Literal(V),
    Quality(PropertyKey),
}
pub(crate) struct Effect {
    pub key: PropertyKey,
    pub op: Op,
    pub arg: Argument,
    pub second: Option<Argument>,
}

pub fn recipe_script_supported(id: u32) -> bool {
    crate::script_table::effects(id).is_some()
}

/// Atomic bounded application of a source embedded recipe script.
pub fn apply_recipe_script(
    properties: &mut BTreeMap<PropertyKey, V>,
    id: u32,
) -> Result<(), CraftError> {
    let effects = crate::script_table::effects(id).ok_or(CraftError::Unsupported)?;
    if properties.len() > 4096 {
        return Err(CraftError::Capacity);
    }
    let mut candidate = properties.clone();
    for effect in effects {
        let Some(arg) = resolve(&effect.arg, &candidate) else {
            continue;
        };
        let old = candidate.get(&effect.key);
        let next = match effect.op {
            Op::Assign => arg,
            Op::AtLeastAdd => match old {
                None => arg,
                Some(old) if less(old, &arg)? => arg,
                Some(old) => {
                    let second = effect.second.as_ref().ok_or(CraftError::InvalidState)?;
                    let Some(second) = resolve(second, &candidate) else {
                        continue;
                    };
                    arithmetic(old, &second, &Op::Add)?
                }
            },
            _ => arithmetic(old.unwrap_or(&zero(effect.key.kind)?), &arg, &effect.op)?,
        };
        if next.kind() != effect.key.kind || !next.valid() {
            return Err(CraftError::InvalidState);
        }
        if candidate.len() == 4096 && !candidate.contains_key(&effect.key) {
            return Err(CraftError::Capacity);
        }
        candidate.insert(effect.key, next);
    }
    *properties = candidate;
    Ok(())
}
fn zero(kind: K) -> Result<V, CraftError> {
    Ok(match kind {
        K::Int => V::Int(0),
        K::Int64 => V::Int64(0),
        K::Float => V::Float(0.0),
        K::Bool => V::Bool(false),
        K::DataId => V::DataId(0),
        _ => return Err(CraftError::Unsupported),
    })
}
fn resolve(arg: &Argument, p: &BTreeMap<PropertyKey, V>) -> Option<V> {
    match arg {
        Argument::Literal(v) => Some(v.clone()),
        Argument::Quality(k) => p.get(k).cloned().or_else(|| match k.kind {
            K::Bool | K::DataId => zero(k.kind).ok(),
            _ => None,
        }),
    }
}
fn less(a: &V, b: &V) -> Result<bool, CraftError> {
    Ok(match (a, b) {
        (V::Int(a), V::Int(b)) => a < b,
        (V::Int64(a), V::Int64(b)) => a < b,
        (V::Float(a), V::Float(b)) => a < b,
        _ => return Err(CraftError::InvalidState),
    })
}
fn arithmetic(a: &V, b: &V, op: &Op) -> Result<V, CraftError> {
    // Integer add/subtract is checked hardening; multiplication/division truncates
    // toward zero, retaining the source double literal and evaluation order.
    match (a, b) {
        (V::Int(a), V::Int(b)) if matches!(op, Op::Add | Op::Subtract) => Ok(V::Int(
            if matches!(op, Op::Add) {
                a.checked_add(*b)
            } else {
                a.checked_sub(*b)
            }
            .ok_or(CraftError::Overflow)?,
        )),
        (V::Int64(a), V::Int64(b)) if matches!(op, Op::Add | Op::Subtract) => Ok(V::Int64(
            if matches!(op, Op::Add) {
                a.checked_add(*b)
            } else {
                a.checked_sub(*b)
            }
            .ok_or(CraftError::Overflow)?,
        )),
        _ => {
            let number = |v: &V| match v {
                V::Int(n) => Ok(f64::from(*n)),
                V::Int64(n) => Ok(*n as f64),
                V::Float(n) => Ok(*n),
                _ => Err(CraftError::InvalidState),
            };
            let (a_num, b_num) = (number(a)?, number(b)?);
            let n = match op {
                Op::Add => a_num + b_num,
                Op::Subtract => a_num - b_num,
                Op::Multiply => a_num * b_num,
                Op::Divide if b_num != 0.0 => a_num / b_num,
                Op::Divide => a_num,
                _ => return Err(CraftError::InvalidState),
            };
            if !n.is_finite() {
                return Err(CraftError::Overflow);
            }
            Ok(match a {
                V::Int(_) if n >= f64::from(i32::MIN) && n <= f64::from(i32::MAX) => {
                    V::Int(n as i32)
                }
                V::Int64(_) if n >= i64::MIN as f64 && n < (i64::MAX as f64) => V::Int64(n as i64),
                V::Float(_) => V::Float(n),
                _ => return Err(CraftError::Overflow),
            })
        }
    }
}
