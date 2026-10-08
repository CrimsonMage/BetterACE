//! Door timers are transient; immutable outputs drain before owner eviction.
use super::*;
impl Doors {
    pub(crate) fn can_retire_region(&self, block: u16) -> bool {
        !self.events.iter().any(|event| {
            self.entries
                .get(&event.door)
                .is_some_and(|entry| entry.prepared.collider.cell.0 >> 16 == u32::from(block))
        })
    }
    pub(crate) fn retire_region(&mut self, block: u16) -> Result<(), DoorRejection> {
        if !self.can_retire_region(block) {
            return Err(DoorRejection::InvalidState);
        }
        self.entries
            .retain(|_, entry| entry.prepared.collider.cell.0 >> 16 != u32::from(block));
        Ok(())
    }
}
