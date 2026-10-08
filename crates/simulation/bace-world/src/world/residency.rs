//! Residency flags share the physics owner; no copied actor state.
use super::*;
impl World {
    pub fn set_region_dormant(&mut self, landblock: u16, dormant: bool) -> Result<(), WorldError> {
        if dormant {
            if self.dormant_landblocks.len() >= 4096
                && !self.dormant_landblocks.contains(&landblock)
            {
                return Err(WorldError::InvalidMotion);
            }
            self.dormant_landblocks.insert(landblock);
        } else {
            self.dormant_landblocks.remove(&landblock);
        }
        Ok(())
    }
    pub fn region_dormant(&self, landblock: u16) -> bool {
        self.dormant_landblocks.contains(&landblock)
    }
    pub fn actor_region_dormant(&self, actor: EntityId) -> bool {
        self.actors
            .get(&actor)
            .is_some_and(|a| self.region_dormant((a.cell.0 >> 16) as u16))
    }
}
impl World {
    pub fn region_has_projectiles(&self, landblock: u16) -> bool {
        self.projectiles
            .values()
            .any(|p| p.cell.0 >> 16 == u32::from(landblock))
    }
    pub fn can_evict_region_after(&self, landblock: u16, removed: &[EntityId]) -> bool {
        !self.region_has_projectiles(landblock)
            && self
                .actors
                .values()
                .all(|a| a.cell.0 >> 16 != u32::from(landblock) || removed.contains(&a.id))
    }
    /// No live actor/projectile may lose collision assets. This releases only
    /// region-owned static data after the gameplay owners completed their drain.
    pub fn evict_empty_region(&mut self, landblock: u16) -> Result<(), WorldError> {
        if !self.can_evict_region_after(landblock, &[]) {
            return Err(WorldError::InvalidMotion);
        }
        self.geometry = self
            .geometry
            .as_ref()
            .and_then(|g| g.without_landblock(landblock))
            .map(std::sync::Arc::new);
        self.scenes
            .retain(|cell, _| cell.0 >> 16 != u32::from(landblock));
        self.doors
            .retain(|_, door| door.cell.0 >> 16 != u32::from(landblock));
        self.dormant_landblocks.remove(&landblock);
        self.remove_visibility_region(landblock);
        Ok(())
    }
}
