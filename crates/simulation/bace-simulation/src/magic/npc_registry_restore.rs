//! Empty freshly admitted registry owners may adopt an exact durable snapshot.
//! Active effects/output are never discarded to make a restore fit.
use super::*;
impl Magic {
    pub(crate) fn preflight_empty_registry_restore(
        &self,
        identities: impl IntoIterator<Item = EntityId>,
    ) -> Result<(), CastRejection> {
        for id in identities {
            if self
                .registries
                .get(&id)
                .is_some_and(|r| !r.entries().is_empty() || r.revision() != 0)
                || self
                    .registry_clocks
                    .get(&id)
                    .is_some_and(|c| c.reserved || c.error.is_some())
                || self
                    .attempts
                    .values()
                    .chain(self.instant_continuations.values())
                    .any(|a| a.origin.actor() == id || a.target == Some(id))
                || self.periodic_involves(id)
                || self.proc_involves(id)
                || self
                    .events
                    .iter()
                    .any(|event| registry::event_actor(event) == id)
            {
                return Err(CastRejection::Busy);
            }
        }
        Ok(())
    }
    pub(crate) fn adopt_empty_registry_restore(
        &mut self,
        registries: Vec<(EntityId, EnchantmentRegistry)>,
    ) {
        for (id, registry) in registries {
            // Admission already created every exact actor/equipment registry.
            *self
                .registries
                .get_mut(&id)
                .expect("NPC restore owner preflight") = registry;
        }
    }
}
impl Magic {
    pub(crate) fn restore_object_registry(
        &mut self,
        actor: EntityId,
        revision: u64,
        entries: Vec<EnchantmentEntry>,
        world: &World,
        now: f64,
    ) -> Result<(), CastRejection> {
        self.check_time(now)?;
        if world.body(actor).is_err()
            || world.properties(actor).is_none()
            || world.combatant(actor).is_some()
            || !self.can_retire_npc(actor)
            || self.flying.values().any(|f| f.source == actor)
        {
            return Err(CastRejection::Busy);
        }
        self.preflight_empty_registry_restore([actor])?;
        let registry = EnchantmentRegistry::restore(512, revision, entries)
            .map_err(|_| CastRejection::InvalidState)?;
        if let std::collections::btree_map::Entry::Occupied(mut existing) =
            self.registries.entry(actor)
        {
            existing.insert(registry);
        } else {
            self.register_registry(actor, registry, true, now)
                .map_err(|(error, _)| error)?;
        }
        Ok(())
    }
}
