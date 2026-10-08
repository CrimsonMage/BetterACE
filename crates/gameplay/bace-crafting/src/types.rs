use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftError {
    InvalidState,
    Ownership,
    Busy,
    Untrained,
    TinkerLimit,
    Overflow,
    Capacity,
    Duplicate,
    StaleConfirmation,
    Requirement,
    Unsupported,
    Random,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PropertyKind {
    Bool,
    Int,
    Int64,
    Float,
    String,
    DataId,
    InstanceId,
    SpellBook,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PropertyKey {
    pub kind: PropertyKind,
    pub id: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Bool(bool),
    Int(i32),
    Int64(i64),
    Float(f64),
    String(String),
    DataId(u32),
    InstanceId(u32),
    SpellBook(bool),
}
impl PropertyValue {
    pub fn kind(&self) -> PropertyKind {
        match self {
            Self::Bool(_) => PropertyKind::Bool,
            Self::Int(_) => PropertyKind::Int,
            Self::Int64(_) => PropertyKind::Int64,
            Self::Float(_) => PropertyKind::Float,
            Self::String(_) => PropertyKind::String,
            Self::DataId(_) => PropertyKind::DataId,
            Self::InstanceId(_) => PropertyKind::InstanceId,
            Self::SpellBook(_) => PropertyKind::SpellBook,
        }
    }
    pub(crate) fn valid(&self) -> bool {
        match self {
            Self::Float(v) => v.is_finite(),
            Self::String(v) => v.len() <= 4096,
            _ => true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Participant {
    Actor,
    Source,
    Target,
}

/// Positive predicates. Importers invert ACE CompareType failure predicates;
/// e.g. ACE GreaterThan becomes AtMost, and ACE NotExist becomes Present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequirementComparison {
    Present,
    Absent,
    Equal,
    NotEqual,
    Less,
    AtMost,
    Greater,
    AtLeast,
    HasAnyBits,
    DoesNotHaveAllBits,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Requirement {
    pub participant: Participant,
    pub key: PropertyKey,
    pub comparison: RequirementComparison,
    pub value: PropertyValue,
    /// ACE numeric/string comparisons generally treat absence as zero/empty.
    pub absent_is_default: bool,
}
impl Requirement {
    pub(crate) fn check(
        &self,
        properties: &BTreeMap<PropertyKey, PropertyValue>,
    ) -> Result<(), CraftError> {
        use RequirementComparison as C;
        let found = properties.get(&self.key);
        if self.key.id == 0 || self.key.kind != self.value.kind() || !self.value.valid() {
            return Err(CraftError::InvalidState);
        }
        let result = match self.comparison {
            C::Present => found.is_some(),
            C::Absent => found.is_none(),
            comparison => {
                let default = match self.key.kind {
                    PropertyKind::SpellBook => PropertyValue::SpellBook(false),
                    PropertyKind::Bool => PropertyValue::Bool(false),
                    PropertyKind::Int => PropertyValue::Int(0),
                    PropertyKind::Int64 => PropertyValue::Int64(0),
                    PropertyKind::Float => PropertyValue::Float(0.0),
                    PropertyKind::String => PropertyValue::String(String::new()),
                    PropertyKind::DataId => PropertyValue::DataId(0),
                    PropertyKind::InstanceId => PropertyValue::InstanceId(0),
                };
                let lhs = found
                    .or(if self.absent_is_default {
                        Some(&default)
                    } else {
                        None
                    })
                    .ok_or(CraftError::Requirement)?;
                let order = match (lhs, &self.value) {
                    (PropertyValue::Bool(a), PropertyValue::Bool(b))
                    | (PropertyValue::SpellBook(a), PropertyValue::SpellBook(b)) => Some(a.cmp(b)),
                    (PropertyValue::Int(a), PropertyValue::Int(b)) => Some(a.cmp(b)),
                    (PropertyValue::Int64(a), PropertyValue::Int64(b)) => Some(a.cmp(b)),
                    (PropertyValue::Float(a), PropertyValue::Float(b)) => a.partial_cmp(b),
                    (PropertyValue::String(a), PropertyValue::String(b)) => Some(a.cmp(b)),
                    (PropertyValue::DataId(a), PropertyValue::DataId(b))
                    | (PropertyValue::InstanceId(a), PropertyValue::InstanceId(b)) => {
                        Some(a.cmp(b))
                    }
                    _ => return Err(CraftError::InvalidState),
                }
                .ok_or(CraftError::InvalidState)?;
                match comparison {
                    C::Equal => order.is_eq(),
                    C::NotEqual => !order.is_eq(),
                    C::Less => order.is_lt(),
                    C::AtMost => !order.is_gt(),
                    C::Greater => order.is_gt(),
                    C::AtLeast => !order.is_lt(),
                    C::HasAnyBits | C::DoesNotHaveAllBits => {
                        let (a, b) = match (lhs, &self.value) {
                            (PropertyValue::Int(a), PropertyValue::Int(b)) => {
                                (*a as u32, *b as u32)
                            }
                            (PropertyValue::DataId(a), PropertyValue::DataId(b))
                            | (PropertyValue::InstanceId(a), PropertyValue::InstanceId(b)) => {
                                (numeric_bits(f64::from(*a)), numeric_bits(f64::from(*b)))
                            }
                            (PropertyValue::Bool(a), PropertyValue::Bool(b)) => {
                                (u32::from(*a), u32::from(*b))
                            }
                            (PropertyValue::Float(a), PropertyValue::Float(b)) => {
                                (numeric_bits(*a), numeric_bits(*b))
                            }
                            _ => return Err(CraftError::Unsupported),
                        };
                        if comparison == C::HasAnyBits {
                            a & b != 0
                        } else {
                            a & b != b
                        }
                    }
                    C::Present | C::Absent => return Err(CraftError::InvalidState),
                }
            }
        };
        if result {
            Ok(())
        } else {
            Err(CraftError::Requirement)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MutationKind {
    /// Active pinned ACE embedded recipe mutation, applied to this participant.
    Script(u32),
    /// AllowedWielder/AllowedActivator source GUID rule.
    CopyIdentity(Participant),
    Set(PropertyValue),
    Remove,
    Add(PropertyValue),
    Multiply(f64),
    SetBitsOn(i32),
    SetBitsOff(i32),
    Copy {
        participant: Participant,
        key: PropertyKey,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Mutation {
    pub participant: Participant,
    pub key: PropertyKey,
    pub kind: MutationKind,
}

pub(crate) fn mutate(
    properties: &mut BTreeMap<PropertyKey, PropertyValue>,
    mutation: &Mutation,
    copied: Option<PropertyValue>,
) -> Result<(), CraftError> {
    if mutation.key.id == 0 {
        return Err(CraftError::InvalidState);
    }
    if let MutationKind::Script(id) = mutation.kind {
        return crate::scripts::apply_recipe_script(properties, id);
    }
    let old = properties.get(&mutation.key);
    let next = match &mutation.kind {
        MutationKind::SetBitsOn(bits) | MutationKind::SetBitsOff(bits) => {
            let old = match old {
                Some(PropertyValue::Int(v)) => *v,
                None => 0,
                _ => return Err(CraftError::Unsupported),
            };
            PropertyValue::Int(if matches!(mutation.kind, MutationKind::SetBitsOn(_)) {
                old | bits
            } else {
                old & !bits
            })
        }
        MutationKind::Remove => {
            properties.remove(&mutation.key);
            return Ok(());
        }
        MutationKind::Set(value) => value.clone(),
        MutationKind::Script(_) => return Err(CraftError::InvalidState),
        MutationKind::CopyIdentity(_) | MutationKind::Copy { .. } => match copied {
            Some(v) => v,
            None => {
                properties.remove(&mutation.key);
                return Ok(());
            }
        },
        MutationKind::Add(value) => match (old, value) {
            (Some(PropertyValue::Int(a)), PropertyValue::Int(b)) => {
                PropertyValue::Int(a.checked_add(*b).ok_or(CraftError::Overflow)?)
            }
            (Some(PropertyValue::Int64(a)), PropertyValue::Int64(b)) => {
                PropertyValue::Int64(a.checked_add(*b).ok_or(CraftError::Overflow)?)
            }
            (Some(PropertyValue::Float(a)), PropertyValue::Float(b)) => PropertyValue::Float(a + b),
            (None, PropertyValue::Int(_) | PropertyValue::Int64(_) | PropertyValue::Float(_)) => {
                value.clone()
            }
            _ => return Err(CraftError::Unsupported),
        },
        MutationKind::Multiply(factor) => {
            if !factor.is_finite() {
                return Err(CraftError::InvalidState);
            }
            match old {
                Some(PropertyValue::Int(v)) => {
                    let n = f64::from(*v) * factor;
                    if n < f64::from(i32::MIN) || n > f64::from(i32::MAX) {
                        return Err(CraftError::Overflow);
                    }
                    PropertyValue::Int(n as i32)
                }
                Some(PropertyValue::Float(v)) => PropertyValue::Float(v * factor),
                None => return Ok(()),
                _ => return Err(CraftError::Unsupported),
            }
        }
    };
    if !next.valid() || next.kind() != mutation.key.kind {
        return Err(CraftError::InvalidState);
    }
    if properties.len() >= 4096 && !properties.contains_key(&mutation.key) {
        return Err(CraftError::Capacity);
    }
    properties.insert(mutation.key, next);
    Ok(())
}

// ACE VerifyRequirement takes double and casts to int before bit tests. The
// pinned .NET x64 conversion returns Int32.MinValue outside its signed range.
fn numeric_bits(value: f64) -> u32 {
    if value >= f64::from(i32::MIN) && value < f64::from(i32::MAX) + 1.0 {
        (value as i32) as u32
    } else {
        i32::MIN as u32
    }
}
