//! Bounded pure respawn queue; inspired by ACE GeneratorProfile SpawnQueue.
//! This owns scheduling/admission, not generator probability or world insertion.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnTicket {
    pub generator: u32,
    pub slot: u32,
    pub generation: u64,
    pub due_tick: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnError {
    Capacity,
    Duplicate,
    TimeOverflow,
}
pub struct SpawnSchedule {
    tickets: Vec<SpawnTicket>,
    capacity: usize,
}
impl SpawnSchedule {
    pub fn new(capacity: usize) -> Result<Self, SpawnError> {
        if !(1..=65536).contains(&capacity) {
            return Err(SpawnError::Capacity);
        }
        Ok(Self {
            tickets: Vec::with_capacity(capacity),
            capacity,
        })
    }
    pub fn schedule(
        &mut self,
        generator: u32,
        slot: u32,
        generation: u64,
        now: u64,
        delay: u64,
    ) -> Result<(), SpawnError> {
        let due_tick = now.checked_add(delay).ok_or(SpawnError::TimeOverflow)?;
        if self
            .tickets
            .iter()
            .any(|t| t.generator == generator && t.slot == slot)
        {
            return Err(SpawnError::Duplicate);
        }
        if self.tickets.len() == self.capacity {
            return Err(SpawnError::Capacity);
        }
        let ticket = SpawnTicket {
            generator,
            slot,
            generation,
            due_tick,
        };
        let index = self
            .tickets
            .partition_point(|t| (t.due_tick, t.generator, t.slot) <= (due_tick, generator, slot));
        self.tickets.insert(index, ticket);
        Ok(())
    }
    /// Retain until the world has capacity and immutable geometry/content is ready.
    pub fn next_due(&self, now: u64) -> Option<SpawnTicket> {
        self.tickets.first().copied().filter(|t| t.due_tick <= now)
    }
    /// Earliest due ticket whose owner can currently make progress. Skipped
    /// tickets retain their original deadline and generation. The scan is
    /// bounded by the schedule capacity and does not allocate or reorder work.
    pub fn next_due_matching(
        &self,
        now: u64,
        mut eligible: impl FnMut(SpawnTicket) -> bool,
    ) -> Option<SpawnTicket> {
        self.tickets
            .iter()
            .copied()
            .take_while(|ticket| ticket.due_tick <= now)
            .find(|ticket| eligible(*ticket))
    }
    /// Consume only the exact admitted generation. Late completions cannot remove
    /// a replacement generator's scheduled work.
    pub fn acknowledge(&mut self, ticket: SpawnTicket) -> bool {
        let Some(index) = self.tickets.iter().position(|current| *current == ticket) else {
            return false;
        };
        self.tickets.remove(index);
        true
    }
    pub fn cancel_generator(&mut self, generator: u32, generation: u64) {
        self.tickets
            .retain(|t| t.generator != generator || t.generation != generation);
    }
    pub fn pending(&self) -> usize {
        self.tickets.len()
    }
}
