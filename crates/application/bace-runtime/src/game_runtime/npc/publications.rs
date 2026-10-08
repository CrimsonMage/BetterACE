//! Cold births hand off their exact script input; publication waits for the
//! simulation's correlated RecoveryReady acknowledgment.
use super::*;
fn same(a: &PreparedNpcRegistration, b: &PreparedNpcRegistration) -> bool {
    a.actor == b.actor
        && a.epoch == b.epoch
        && a.landblock == b.landblock
        && a.script_identity() == b.script_identity()
        && a.source.authored == b.source.authored
        && a.source.properties == b.source.properties
}
impl GameRuntime {
    pub(in crate::game_runtime) fn npc_source_ready(
        &self,
        actor: EntityId,
        identity: bace_simulation::NpcScriptIdentity,
    ) -> bool {
        self.npc
            .definitions
            .get(&actor)
            .is_some_and(|d| d.admitted && d.script_identity() == identity)
            && self.npc.coordinator.source_ready(actor)
    }
    pub(in crate::game_runtime) fn enqueue_generated_npc_registration(
        &mut self,
        registration: PreparedNpcRegistration,
    ) -> Result<(), Box<PreparedNpcRegistration>> {
        if !registration.admitted || !registration.script_source().valid_bounds() {
            return Err(Box::new(registration));
        }
        if let Some(existing) = self.npc.definitions.get(&registration.actor) {
            if same(existing, &registration) {
                return Ok(());
            }
            if existing.epoch >= registration.epoch
                || self
                    .npc
                    .notifications
                    .iter()
                    .any(|n| n.context.source == registration.actor)
                || self.npc.work.contains_key(&registration.actor)
                || self.npc.idle.contains_key(&registration.actor)
                || self
                    .npc
                    .coordinator
                    .release_source(registration.actor)
                    .is_err()
            {
                return Err(Box::new(registration));
            }
            self.npc.definitions.remove(&registration.actor);
        }
        if let Some(existing) = self
            .npc
            .registration
            .as_ref()
            .filter(|r| r.actor == registration.actor)
            .or(self
                .npc
                .head_registration
                .as_ref()
                .filter(|r| r.actor == registration.actor))
            .or_else(|| {
                self.npc
                    .generated_registrations
                    .iter()
                    .find(|r| r.actor == registration.actor)
            })
        {
            return if same(existing, &registration) {
                Ok(())
            } else {
                Err(Box::new(registration))
            };
        }
        let used = self
            .npc
            .generated_registrations
            .iter()
            .try_fold(registration.source.retained_bytes, |n, r| {
                n.checked_add(r.source.retained_bytes)
            });
        if self.npc.generated_registrations.len() >= 256
            || used.is_none_or(|n| n > 64 * 1024 * 1024)
        {
            return Err(Box::new(registration));
        }
        self.npc.generated_registrations.push_back(registration);
        Ok(())
    }
}
