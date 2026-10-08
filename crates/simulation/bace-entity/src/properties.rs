//! Single-owner scalar qualities and mutation-revision projections for actors.
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PropertyFamily {
    Bool,
    Int,
    Int64,
    Float,
    String,
    Attribute,
    RawAttribute,
    Vital,
    RawVital,
    Skill,
    RawSkill,
    SkillAdvancement,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Bool(bool),
    Int(i32),
    Int64(i64),
    Float(f64),
    String(String),
    Unsigned(u32),
}
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyChange {
    pub family: PropertyFamily,
    pub stat: u32,
    pub before: Option<PropertyValue>,
    pub after: Option<PropertyValue>,
    pub before_revision: u64,
    pub after_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyError {
    Invalid,
    Capacity,
    Conflict,
    Overflow,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EntityProperties {
    values: BTreeMap<(PropertyFamily, u32), PropertyValue>,
    revision: u64,
    capacity: usize,
}
impl EntityProperties {
    pub fn retained_bytes(&self) -> Option<usize> {
        self.values.values().try_fold(0usize, |n, v| {
            n.checked_add(match v {
                PropertyValue::String(s) => s.len().checked_add(32)?,
                _ => 32,
            })
        })
    }
    pub fn snapshot(&self) -> (u64, Vec<(PropertyFamily, u32, PropertyValue)>) {
        (
            self.revision,
            self.values
                .iter()
                .map(|(&(family, stat), value)| (family, stat, value.clone()))
                .collect(),
        )
    }
    pub fn restore_snapshot(
        revision: u64,
        values: Vec<(PropertyFamily, u32, PropertyValue)>,
    ) -> Result<Self, PropertyError> {
        if values.len() > 4096 {
            return Err(PropertyError::Capacity);
        }
        let mut state = Self::new(4096)?;
        for (family, stat, value) in values {
            if state.values.contains_key(&(family, stat)) {
                return Err(PropertyError::Invalid);
            }
            let change = state.propose(family, stat, Some(value))?;
            state.adopt(change)?;
        }
        state.revision = revision;
        Ok(state)
    }
    pub fn new(capacity: usize) -> Result<Self, PropertyError> {
        if capacity == 0 || capacity > 4096 {
            return Err(PropertyError::Capacity);
        }
        Ok(Self {
            values: BTreeMap::new(),
            revision: 0,
            capacity,
        })
    }
    pub fn get(&self, family: PropertyFamily, stat: u32) -> Option<&PropertyValue> {
        self.values.get(&(family, stat))
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn propose(
        &self,
        family: PropertyFamily,
        stat: u32,
        after: Option<PropertyValue>,
    ) -> Result<PropertyChange, PropertyError> {
        if stat > 65535 {
            return Err(PropertyError::Invalid);
        }
        if let Some(value) = &after {
            let valid = match (family, value) {
                (PropertyFamily::Bool, PropertyValue::Bool(_))
                | (PropertyFamily::Int, PropertyValue::Int(_))
                | (PropertyFamily::Int64, PropertyValue::Int64(_)) => true,
                (PropertyFamily::Float, PropertyValue::Float(v)) => v.is_finite(),
                (PropertyFamily::String, PropertyValue::String(v)) => v.len() <= 65536,
                (
                    PropertyFamily::Attribute
                    | PropertyFamily::RawAttribute
                    | PropertyFamily::Vital
                    | PropertyFamily::RawVital
                    | PropertyFamily::Skill
                    | PropertyFamily::RawSkill
                    | PropertyFamily::SkillAdvancement,
                    PropertyValue::Unsigned(_),
                ) => true,
                _ => false,
            };
            if !valid {
                return Err(PropertyError::Invalid);
            }
        }
        let before = self.get(family, stat).cloned();
        if before.is_none() && after.is_some() && self.values.len() >= self.capacity {
            return Err(PropertyError::Capacity);
        }
        let after_revision = if before == after {
            self.revision
        } else {
            self.revision
                .checked_add(1)
                .ok_or(PropertyError::Overflow)?
        };
        Ok(PropertyChange {
            family,
            stat,
            before,
            after,
            before_revision: self.revision,
            after_revision,
        })
    }
    pub fn adopt(&mut self, change: PropertyChange) -> Result<(), PropertyError> {
        if self.revision != change.before_revision
            || self.propose(change.family, change.stat, change.after.clone())? != change
        {
            return Err(PropertyError::Conflict);
        }
        if let Some(after) = change.after {
            self.values.insert((change.family, change.stat), after);
        } else {
            self.values.remove(&(change.family, change.stat));
        }
        self.revision = change.after_revision;
        Ok(())
    }
}
