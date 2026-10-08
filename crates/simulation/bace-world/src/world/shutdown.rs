//! Stronger than an empty actor iterator: projectiles, corpses and held owner
//! callbacks must also be gone before the host releases database world ownership.
use super::*;
impl World {
    pub fn has_live_state(&self) -> bool {
        !self.actors.is_empty()
            || !self.combatants.is_empty()
            || !self.projectiles.is_empty()
            || !self.corpses.is_empty()
            || !self.doors.is_empty()
            || !self.properties.is_empty()
            || !self.retirement_holds.is_empty()
            || !self.portal_transit.is_empty()
            || !self.entry_pending.is_empty()
            || !self.npc_admissions.is_empty()
            || !self.vital_reservations.is_empty()
            || !self.motion_events.is_empty()
            || self.pending_health_observation().is_some()
            || self.has_motion_state()
    }
}
