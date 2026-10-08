//! Owner-derived housing/cell access projections. Generation changes invalidate
//! grants immediately; occupants move only through an explicit validated boot.
use super::*;
impl World {
    pub fn set_restriction_generation(
        &mut self,
        restriction: u32,
        generation: u64,
    ) -> Result<(), WorldError> {
        if restriction == 0
            || generation == 0
            || self
                .restriction_generations
                .get(&restriction)
                .is_some_and(|old| *old > generation)
            || self.restriction_generations.len() >= 65536
                && !self.restriction_generations.contains_key(&restriction)
        {
            return Err(WorldError::InvalidCellAccess);
        }
        self.restriction_generations.insert(restriction, generation);
        Ok(())
    }
    /// Lifecycle may prepare a grant before actor admission. Only the simulation
    /// owner calls this after account/house policy and lease checks; no packet
    /// grants access. Unknown restriction generations are denied.
    pub fn set_cell_access(
        &mut self,
        actor: EntityId,
        restriction: u32,
        generation: u64,
        allowed: bool,
    ) -> Result<(), WorldError> {
        if actor.0 == 0 || self.restriction_generations.get(&restriction) != Some(&generation) {
            return Err(WorldError::InvalidCellAccess);
        }
        if !allowed {
            if let Some(grants) = self.cell_access.get_mut(&actor) {
                grants.remove(&restriction);
                if grants.is_empty() {
                    self.cell_access.remove(&actor);
                }
            }
            return Ok(());
        }
        if self.cell_access.len() >= 4096 && !self.cell_access.contains_key(&actor) {
            return Err(WorldError::InvalidCellAccess);
        }
        let grants = self.cell_access.entry(actor).or_default();
        if grants.len() >= 64 && !grants.contains_key(&restriction) {
            return Err(WorldError::InvalidCellAccess);
        }
        grants.insert(restriction, generation);
        Ok(())
    }
    pub fn has_cell_access(&self, actor: EntityId, restriction: u32) -> bool {
        self.cell_access
            .get(&actor)
            .and_then(|v| v.get(&restriction))
            .is_some_and(|generation| {
                self.restriction_generations.get(&restriction) == Some(generation)
            })
    }
    /// Call for player loading/spawn as well as explicit teleports. The geometry
    /// query is read-only; absence cannot implicitly open a housing barrier.
    pub fn validate_player_cell_entry(
        &self,
        actor: EntityId,
        cell: CellId,
    ) -> Result<(), WorldError> {
        let region = self.geometry.as_ref().ok_or(WorldError::MissingGeometry)?;
        let geometry = region.cell(cell.0).ok_or(WorldError::MissingGeometry)?;
        if let Some(restriction) = geometry.restriction
            && !self.has_cell_access(actor, restriction)
        {
            return Err(
                PhysicsError::Geometry(bace_physics::GeometryError::Restricted(restriction)).into(),
            );
        }
        Ok(())
    }
}
