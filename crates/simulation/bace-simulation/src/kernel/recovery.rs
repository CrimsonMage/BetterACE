//! Physical and spell recovery independently dirty the owning character.
use super::*;
impl Kernel {
    pub(super) fn sync_recovery_revisions(&mut self) -> Result<(), SimulationError> {
        let now = self.tick as f64 / 30.0;
        for (actor, previous) in &mut self.physical_recovery_deadlines {
            // A prepared valuable operation already owns the aggregate revision.
            // Leave the old marker intact so recovery changes remain owed afterward.
            if self.characters.reserved(*actor)
                || self.npcs.reserved(*actor)
                || self.inventory.reserved(*actor)
                || self.housing.reserved(*actor)
                || self.pets.reserved(*actor)
                || self.portals.reserved(*actor)
            {
                continue;
            }

            let remaining = self.combat.capture_physical_recovery(*actor, now);
            let deadline = now + remaining;
            if remaining > 0.0
                && deadline > *previous + 0.000001
                && (self.characters.get(*actor).is_none()
                    || self
                        .characters
                        .touch_auxiliary(*actor)
                        .map_err(|_| SimulationError::AuxiliaryRevision)?)
            {
                *previous = deadline;
            }
        }
        for (actor, seen) in &mut self.recovery_revisions {
            if self.characters.reserved(*actor)
                || self.npcs.reserved(*actor)
                || self.inventory.reserved(*actor)
                || self.housing.reserved(*actor)
                || self.pets.reserved(*actor)
                || self.portals.reserved(*actor)
            {
                continue;
            }
            let Some(revision) = self.magic.recovery_revision(*actor) else {
                continue;
            };
            if revision != *seen
                && (self.characters.get(*actor).is_none()
                    || self
                        .characters
                        .touch_auxiliary(*actor)
                        .map_err(|_| SimulationError::AuxiliaryRevision)?)
            {
                *seen = revision;
            }
        }
        Ok(())
    }
}
