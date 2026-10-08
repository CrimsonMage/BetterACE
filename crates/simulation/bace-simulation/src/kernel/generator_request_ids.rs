//! Atomic, request-specific GUID reservations. Source materialization and retries
//! retain their identity even while other generators request fresh initial IDs.
use super::*;
use crate::GeneratorServiceError as G;
use bace_gameplay_api::GeneratorSpawnKey;
use std::collections::BTreeSet;
impl Kernel {
    /// No object has ever used these allocated identities. Permanent allocator
    /// gaps are safe; identities are never recycled. All spawn owners must drain.
    pub fn discard_unused_generator_ids_after_drain(&mut self) -> Result<(), G> {
        if !self.generators.machines.is_empty()
            || !self.generators.requests.is_empty()
            || !self.generators.submitted.is_empty()
            || !self.generators.effects.is_empty()
            || !self.generators.lifecycle_frames.is_empty()
        {
            return Err(G::Busy);
        }
        self.generators.ids.clear();
        Ok(())
    }
    pub(super) fn available_generator_id(&self, id: EntityId) -> bool {
        (0x80000000..=0xfffffffe).contains(&id.0)
            && !self.world.contains_identity(id)
            && !self.population.reserves_identity(id)
            && !self.magic.reserves_identity(id)
            && self.inventory.item(id).is_none()
            && !self.generators.reserves(id)
            && !self.generated_vendor_reserves_identity(id)
            && !self.combat.reserves_projectile(id)
    }
    /// `expected` is the immutable original prefix length, not the current length
    /// guessed on retry. Repeating the same suffix succeeds without consuming IDs.
    pub fn supply_generator_request_ids(
        &mut self,
        key: GeneratorSpawnKey,
        expected: usize,
        ids: &[EntityId],
    ) -> Result<(), G> {
        let total = expected.checked_add(ids.len()).ok_or(G::Capacity)?;
        if expected == 0 || ids.is_empty() || total > 1024 {
            return Err(G::Invalid);
        }
        let request = self.generators.requests.get(&key).ok_or(G::Stale)?;
        if request.entities.len() == total && request.entities[expected..] == *ids {
            return Ok(());
        }
        if request.entities.len() != expected {
            return Err(G::Stale);
        }
        let mut seen = BTreeSet::new();
        if ids
            .iter()
            .any(|&id| !seen.insert(id) || !self.available_generator_id(id))
        {
            return Err(G::Invalid);
        }
        self.generators
            .requests
            .get_mut(&key)
            .expect("validated request")
            .entities
            .extend_from_slice(ids);
        Ok(())
    }
}
