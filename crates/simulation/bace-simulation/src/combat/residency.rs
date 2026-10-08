//! Retire only launched physical missiles; uncommitted ammunition proposals
//! stay with their valuable-operation owner and continue to block teardown.
use super::*;
impl Combat {
    pub(crate) fn retire_region_projectiles(&mut self, world: &mut World, landblock: u16) -> bool {
        let ids: Vec<_> = self
            .missiles
            .iter()
            .filter(|(_, state)| {
                state.launched
                    && state.pending_contact.is_none()
                    && world
                        .projectile(EntityId(state.proposal.projectile))
                        .is_some_and(|p| p.cell.0 >> 16 == u32::from(landblock))
            })
            .map(|(&operation, state)| (operation, EntityId(state.proposal.projectile)))
            .take(32)
            .collect();
        for (operation, id) in ids {
            if self.physical_events.len() >= self.capacity.max(2) {
                return false;
            }
            self.missiles.remove(&operation);
            world.remove_projectile(id);
            self.physical_events
                .push_back(PhysicalCombatEvent::ProjectileRemoved {
                    actor: id,
                    tick: self.simulation_tick,
                });
        }
        !self.missiles.values().any(|state| {
            world
                .projectile(EntityId(state.proposal.projectile))
                .is_some_and(|p| p.cell.0 >> 16 == u32::from(landblock))
        })
    }
}

impl Combat {
    /// Caller proves the world and character owners are empty. Prepared assets
    /// and unused identities are dispensable only after all effects are drained.
    pub(crate) fn clear_idle_physical_assets(&mut self) -> bool {
        if !self.attacks.is_empty()
            || !self.physical_attacks.is_empty()
            || self
                .physical_drivers
                .values()
                .any(driver::AttackDriver::active)
            || !self.missiles.is_empty()
            || !self.launches.is_empty()
            || !self.physical_hits.is_empty()
            || !self.events.is_empty()
            || !self.physical_events.is_empty()
            || !self.dirty.is_empty()
            || self.physical_motion_pending.is_some()
        {
            return false;
        }
        self.physical.clear();
        self.physical_ratings.clear();
        self.offhand.clear();
        self.physical_deadlines.clear();
        self.physical_motions.clear();
        self.physical_options.clear();
        self.physical_sources.clear();
        self.physical_drivers.clear();
        self.physical_driver_cursor = None;
        self.physical_approach_scratch.clear();
        self.missile_ids.clear();
        self.skills.clear();
        true
    }
}
