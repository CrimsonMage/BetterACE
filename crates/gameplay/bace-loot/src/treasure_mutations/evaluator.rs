//! ACE mutation draw sharing, typed arithmetic and source effect ordering.
use super::*;
use crate::treasure_random::unit;
use bace_content::Property;
#[derive(Clone, Copy)]
enum Value {
    Int(i32),
    Float(f64),
}
impl Value {
    fn double(self) -> f64 {
        match self {
            Self::Int(v) => f64::from(v),
            Self::Float(v) => v,
        }
    }
    fn int(self) -> Result<i32, TreasureError> {
        match self {
            Self::Int(v) => Ok(v),
            Self::Float(v) => {
                if !v.is_finite() || v.trunc() < i32::MIN as f64 || v.trunc() > i32::MAX as f64 {
                    return Err(TreasureError::Bounds);
                }
                Ok(v as i32)
            }
        }
    }
    fn arithmetic(self, b: Self, op: Operation) -> Result<Self, TreasureError> {
        let double = match op {
            Operation::Add => self.double() + b.double(),
            Operation::Subtract => self.double() - b.double(),
            Operation::Multiply => self.double() * b.double(),
            Operation::Divide => {
                if b.double() == 0.0 {
                    self.double()
                } else {
                    self.double() / b.double()
                }
            }
            _ => return Err(TreasureError::Bounds),
        };
        if !double.is_finite() {
            return Err(TreasureError::Bounds);
        }
        match (self, b) {
            (Self::Int(a), Self::Int(b)) => Ok(Self::Int(match op {
                Operation::Add => a.wrapping_add(b),
                Operation::Subtract => a.wrapping_sub(b),
                Operation::Multiply => a.wrapping_mul(b),
                Operation::Divide => {
                    if b == 0 {
                        a
                    } else {
                        a.checked_div(b).ok_or(TreasureError::Bounds)?
                    }
                }
                _ => return Err(TreasureError::Bounds),
            })),
            (Self::Int(_), Self::Float(_)) => Ok(Self::Int(Self::Float(double).int()?)),
            _ => Ok(Self::Float(double)),
        }
    }
    fn compare(self, b: Self, greater: bool) -> bool {
        match (self, b) {
            (Self::Int(a), Self::Int(b)) => {
                if greater {
                    a > b
                } else {
                    a < b
                }
            }
            (Self::Float(a), Self::Float(b)) => {
                if greater {
                    a > b
                } else {
                    a < b
                }
            }
            _ => false,
        }
    }
}
fn read(q: Quality, item: &WeenieV1) -> (Value, bool) {
    match q {
        Quality::Int(id) => {
            let v = item
                .properties
                .ints
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value);
            (Value::Int(v.unwrap_or(0)), v.is_some())
        }
        Quality::Float(id) => {
            let v = item
                .properties
                .floats
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value);
            (Value::Float(v.unwrap_or(0.)), v.is_some())
        }
    }
}
fn resolve<R: TreasureRandom>(
    arg: Argument,
    item: &WeenieV1,
    random: &mut R,
) -> Result<(Value, bool), TreasureError> {
    Ok(match arg {
        Argument::Int(v) => (Value::Int(v), true),
        Argument::Float(v) => (Value::Float(v), true),
        Argument::Quality(q) => read(q, item),
        Argument::Random(a, b) => (
            Value::Float(unit(random)? * f64::from(b - a) + f64::from(a)),
            true,
        ),
    })
}
fn store<T>(props: &mut Vec<Property<T>>, id: u32, value: T) -> Result<(), TreasureError> {
    if let Some(p) = props.iter_mut().find(|p| p.id == id) {
        p.value = value;
    } else {
        if props.len() >= 4096 {
            return Err(TreasureError::Capacity);
        }
        props.push(Property { id, value });
        props.sort_by_key(|p| p.id);
    }
    Ok(())
}
impl MutationScript {
    pub fn apply<R: TreasureRandom>(
        &self,
        item: &mut WeenieV1,
        tier: i32,
        random: &mut R,
    ) -> Result<bool, TreasureError> {
        let mut next = item.clone();
        let mut cursor = random.clone();
        let roll = unit(&mut cursor)?;
        let mut mutated = false;
        for m in &self.mutations {
            let tier = if m.chances.len() >= 6 && tier > m.chances.len() as i32 {
                m.chances.len() as i32
            } else {
                tier
            };
            if tier < 1
                || tier > m.chances.len() as i32
                || roll >= f64::from(m.chances[tier as usize - 1])
            {
                continue;
            }
            let roll = unit(&mut cursor)?;
            for outcome in &m.outcomes {
                if let Some(list) = outcome.iter().find(|e| roll < f64::from(e.chance)) {
                    for effect in &list.effects {
                        mutated |= effect.apply(&mut next, &mut cursor)?;
                    }
                }
            }
        }
        *item = next;
        *random = cursor;
        Ok(mutated)
    }
}
impl Effect {
    fn apply<R: TreasureRandom>(
        &self,
        item: &mut WeenieV1,
        random: &mut R,
    ) -> Result<bool, TreasureError> {
        use Operation::*;
        let (result, valid) = read(self.quality, item);
        let (a, valid_a) = resolve(self.arg1, item, random)?;
        let b = self.arg2.map(|b| resolve(b, item, random)).transpose()?;
        if !valid_a || b.is_some_and(|(_, valid)| !valid) {
            return Ok(false);
        }
        let require_b = || b.map(|(b, _)| b).ok_or(TreasureError::Bounds);
        let result = match self.operation {
            Assign => a,
            Add | Subtract | Multiply | Divide => result.arithmetic(a, self.operation)?,
            AtLeastAdd => {
                if !valid || result.compare(a, false) {
                    a
                } else {
                    result.arithmetic(require_b()?, Add)?
                }
            }
            AtMostSubtract => {
                if !valid || result.compare(a, true) {
                    a
                } else {
                    result.arithmetic(require_b()?, Subtract)?
                }
            }
            AddMultiply => result.arithmetic(a.arithmetic(require_b()?, Multiply)?, Add)?,
            AddDivide => result.arithmetic(a.arithmetic(require_b()?, Divide)?, Add)?,
            SubtractMultiply => {
                result.arithmetic(a.arithmetic(require_b()?, Multiply)?, Subtract)?
            }
            SubtractDivide => result.arithmetic(a.arithmetic(require_b()?, Divide)?, Subtract)?,
            AssignAdd => a.arithmetic(require_b()?, Add)?,
            AssignSubtract => a.arithmetic(require_b()?, Subtract)?,
            // Pinned ACE AssignDivide intentionally invokes multiplication.
            AssignMultiply | AssignDivide => a.arithmetic(require_b()?, Multiply)?,
        };
        match self.quality {
            Quality::Int(id) => store(&mut item.properties.ints, id, result.int()?)?,
            Quality::Float(id) => store(&mut item.properties.floats, id, result.double())?,
        };
        Ok(true)
    }
}
