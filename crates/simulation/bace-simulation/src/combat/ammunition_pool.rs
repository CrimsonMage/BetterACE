//! Atomic admission of allocator-owned projectile identity batches.
use super::*;
impl Combat {
    pub(crate) fn available_missile_ids(&self) -> usize {
        self.missile_ids.len()
    }
    pub(crate) fn preflight_missile_ids(
        &self,
        ids: &[EntityId],
        world: &World,
    ) -> Result<(), CombatRejection> {
        if ids.len() > self.capacity.saturating_sub(self.missile_ids.len()) {
            return Err(CombatRejection::Capacity);
        }
        if ids.windows(2).any(|p| p[0] >= p[1])
            || ids.iter().any(|id| {
                id.0 == 0 || world.contains_identity(*id) || self.reserves_projectile(*id)
            })
        {
            return Err(CombatRejection::InvalidRequest);
        }
        Ok(())
    }
}
