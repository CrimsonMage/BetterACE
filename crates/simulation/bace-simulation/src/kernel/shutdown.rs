//! Trusted terminal cleanup after the runtime has drained every durable lane.
//! Cold assets and committed allegiance baselines are dispensable; active owner
//! state, pending receipts and output are never discarded by this method.
use super::*;
impl Kernel {
    pub fn discard_idle_preparation(&mut self) -> Result<(), &'static str> {
        if self.world.has_live_state() || self.has_characters() || !self.commands.is_empty() {
            return Err("world, characters or command admission is not empty");
        }
        if !self.combat.clear_idle_physical_assets() {
            return Err("physical combat retains work");
        }
        if !self.magic.clear_idle_assets(&self.world) {
            return Err("magic retains work");
        }
        self.discard_idle_npc_sources()
            .map_err(|_| "NPC source retains work")?;
        let allegiance = &self.allegiances;
        if allegiance.pending.is_some()
            || allegiance.submitted
            || allegiance.checkpoint.is_some()
            || allegiance.cache_operation.is_some()
            || allegiance.context.is_some()
            || allegiance.pve_operation.is_some()
            || !allegiance.registries.is_empty()
            || !allegiance.experience_events.is_empty()
            || !allegiance.cached_skills.is_empty()
            || !allegiance.listeners.is_empty()
        {
            return Err("allegiance retains uncommitted work");
        }
        // All forest mutation paths adopt only after their combined durable
        // receipt. The runtime must first drain AllegianceService and egress.
        self.allegiances = crate::allegiances::Allegiances::new(self.allegiances.capacity);
        Ok(())
    }
}
