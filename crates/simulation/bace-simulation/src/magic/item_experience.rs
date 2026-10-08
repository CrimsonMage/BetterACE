//! Durable equipment set changes retain exact registries until commit.
use super::*;
use crate::ItemExperienceRegistryChange;
impl Magic {
    pub(crate) fn validate_item_experience_registries(
        &self,
        patches: &[ItemExperienceRegistryChange],
    ) -> Result<(), CastRejection> {
        let count = patches
            .iter()
            .try_fold(0usize, |n, p| n.checked_add(p.events.len()))
            .ok_or(CastRejection::Capacity)?;
        if count > self.capacity - self.events.len() {
            return Err(CastRejection::Capacity);
        }
        self.validate_equipment_registries(patches)
    }
    /// Equipment publication is retained by its inventory operation.
    pub(crate) fn validate_equipment_registries(
        &self,
        patches: &[ItemExperienceRegistryChange],
    ) -> Result<(), CastRejection> {
        for patch in patches {
            let registry = self
                .registries
                .get(&patch.actor)
                .ok_or(CastRejection::MissingActor)?;
            if registry.revision() != patch.before_revision
                || registry.entries() != patch.before
                || !self.registry_reserved(patch.actor)
                || self.registry_failure(patch.actor).is_some()
            {
                return Err(CastRejection::Busy);
            }
            EnchantmentRegistry::restore(patch.capacity, patch.after_revision, patch.after.clone())
                .map_err(|_| CastRejection::InvalidState)?;
        }
        Ok(())
    }
    pub(crate) fn adopt_equipment_registries(&mut self, patches: &[ItemExperienceRegistryChange]) {
        for patch in patches {
            let registry = EnchantmentRegistry::restore(
                patch.capacity,
                patch.after_revision,
                patch.after.clone(),
            )
            .expect("validated equipment registry");
            self.registries.insert(patch.actor, registry);
        }
    }
    pub(crate) fn adopt_item_experience_registries(
        &mut self,
        patches: &[ItemExperienceRegistryChange],
    ) {
        for patch in patches {
            let registry = EnchantmentRegistry::restore(
                patch.capacity,
                patch.after_revision,
                patch.after.clone(),
            )
            .expect("validated item XP registry");
            self.registries.insert(patch.actor, registry);
            self.events.extend(patch.events.iter().cloned());
        }
    }
}
