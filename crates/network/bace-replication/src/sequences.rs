//! Pinned ACE SequenceManager semantics, with bounded collision-free keys.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum SequenceKind {
    ObjectPosition = 0,
    ObjectMovement,
    ObjectState,
    ObjectVector,
    ObjectTeleport,
    ObjectServerControl,
    ObjectForcePosition,
    ObjectVisualDesc,
    ObjectInstance,
    Motion,
    PropertyInt,
    PropertyInt64,
    PropertyBool,
    PropertyDouble,
    PropertyDataId,
    PropertyInstanceId,
    PropertyString,
    RestrictionDb,
    Attribute,
    Vital,
    Position,
    Skill,
}
impl SequenceKind {
    fn initial(self) -> u16 {
        if self == Self::Motion {
            1
        } else if (self as u16) < 9 {
            0
        } else {
            255
        }
    }
    fn maximum(self) -> u16 {
        if self == Self::Motion {
            0x7fff
        } else if (self as u16) < 9 {
            u16::MAX
        } else {
            255
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicationError {
    InvalidCapacity,
    Capacity,
    ClockOverflow,
    InvalidClock,
    GenerationExhausted,
}

pub struct Sequences {
    values: BTreeMap<(SequenceKind, u32), u16>,
    capacity: usize,
}
impl Sequences {
    pub fn new(capacity: usize) -> Result<Self, ReplicationError> {
        if !(1..=65536).contains(&capacity) {
            return Err(ReplicationError::InvalidCapacity);
        }
        Ok(Self {
            values: BTreeMap::new(),
            capacity,
        })
    }
    pub fn current(&self, kind: SequenceKind, property: u32) -> u16 {
        self.values
            .get(&(kind, property))
            .copied()
            .unwrap_or_else(|| kind.initial())
    }
    /// Reserve all distinct keys before advancing any sequence. A rejected
    /// multi-message projection cannot consume a prefix of its counters.
    pub fn advance_batch<const N: usize>(
        &mut self,
        keys: [(SequenceKind, u32); N],
    ) -> Result<[u16; N], ReplicationError> {
        let fresh = keys
            .iter()
            .enumerate()
            .filter(|(i, key)| !self.values.contains_key(key) && !keys[..*i].contains(key))
            .count();
        if self
            .values
            .len()
            .checked_add(fresh)
            .is_none_or(|size| size > self.capacity)
        {
            return Err(ReplicationError::Capacity);
        }
        Ok(std::array::from_fn(|i| {
            self.advance(keys[i].0, keys[i].1)
                .expect("reserved sequence capacity")
        }))
    }
    pub fn advance(&mut self, kind: SequenceKind, property: u32) -> Result<u16, ReplicationError> {
        if !self.values.contains_key(&(kind, property)) && self.values.len() == self.capacity {
            return Err(ReplicationError::Capacity);
        }
        let value = self.current(kind, property);
        let next = if value == kind.maximum() {
            0
        } else {
            value + 1
        };
        self.values.insert((kind, property), next);
        Ok(next)
    }
}
