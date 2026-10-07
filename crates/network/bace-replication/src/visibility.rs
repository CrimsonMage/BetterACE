//! Per-observer knowledge; callers supply authoritative visibility decisions.
//! Late forget work cannot remove an entity after a newer observe/re-entry.
use crate::ReplicationError;
use bace_types::EntityId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForgetTicket {
    entity: EntityId,
    generation: u64,
    deadline_ms: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityChange {
    Create,
    Retained,
}
struct Known {
    generation: u64,
    pending: Option<ForgetTicket>,
}
pub struct Visibility {
    known: BTreeMap<EntityId, Known>,
    capacity: usize,
    generation: u64,
    last_ms: u64,
}
impl Visibility {
    pub fn new(capacity: usize) -> Result<Self, ReplicationError> {
        if !(1..=65536).contains(&capacity) {
            return Err(ReplicationError::InvalidCapacity);
        }
        Ok(Self {
            known: BTreeMap::new(),
            capacity,
            generation: 0,
            last_ms: 0,
        })
    }
    /// A Create result obliges the adapter to enqueue a complete description;
    /// output overload must close the peer, never silently mark it synchronized.
    pub fn observe(
        &mut self,
        entity: EntityId,
        now_ms: u64,
    ) -> Result<VisibilityChange, ReplicationError> {
        self.clock(now_ms)?;
        if !self.known.contains_key(&entity) && self.known.len() == self.capacity {
            return Err(ReplicationError::Capacity);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(ReplicationError::GenerationExhausted)?;
        let change = if self.known.contains_key(&entity) {
            VisibilityChange::Retained
        } else {
            VisibilityChange::Create
        };
        self.known.insert(
            entity,
            Known {
                generation,
                pending: None,
            },
        );
        self.generation = generation;
        Ok(change)
    }
    pub fn forget_later(
        &mut self,
        entity: EntityId,
        now_ms: u64,
        delay_ms: u64,
    ) -> Result<Option<ForgetTicket>, ReplicationError> {
        self.clock(now_ms)?;
        let deadline_ms = now_ms
            .checked_add(delay_ms)
            .ok_or(ReplicationError::ClockOverflow)?;
        let Some(known) = self.known.get_mut(&entity) else {
            return Ok(None);
        };
        // Repeated out-of-view observations must not indefinitely postpone removal.
        let ticket = *known.pending.get_or_insert(ForgetTicket {
            entity,
            generation: known.generation,
            deadline_ms,
        });
        Ok(Some(ticket))
    }
    /// Returns true only when removal committed. The caller then encodes the
    /// remove message; a stale ticket never removes a newer create/observation.
    pub fn commit_forget(
        &mut self,
        ticket: ForgetTicket,
        now_ms: u64,
    ) -> Result<bool, ReplicationError> {
        self.clock(now_ms)?;
        let matches = self
            .known
            .get(&ticket.entity)
            .is_some_and(|k| k.generation == ticket.generation && k.pending == Some(ticket));
        if now_ms < ticket.deadline_ms || !matches {
            return Ok(false);
        }
        self.known.remove(&ticket.entity);
        Ok(true)
    }
    pub fn knows(&self, entity: EntityId) -> bool {
        self.known.contains_key(&entity)
    }
    pub fn len(&self) -> usize {
        self.known.len()
    }
    pub fn is_empty(&self) -> bool {
        self.known.is_empty()
    }
    fn clock(&mut self, now_ms: u64) -> Result<(), ReplicationError> {
        if now_ms < self.last_ms {
            return Err(ReplicationError::InvalidClock);
        }
        self.last_ms = now_ms;
        Ok(())
    }
}
