//! Transfer the legacy empty-corpse timer to the complete durable expiry owner.
use super::*;
impl Population {
    pub(crate) fn handoff_corpse_expiry(&mut self, corpse: EntityId, operation: u64) {
        self.decays.cancel_generator(corpse.0, operation);
    }
}
