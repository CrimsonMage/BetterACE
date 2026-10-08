//! Changed capabilities refresh the existing accepted interpreter. No DAT work
//! or full inventory scan occurs for unchanged movement on successive ticks.
use super::*;
impl Kernel {
    pub(super) fn refresh_dirty_locomotion(&mut self) -> Result<(), SimulationError> {
        while let Some(actor) = self.inventory.take_burden_dirty() {
            if self.characters.get(actor).is_some() {
                self.locomotion_dirty.insert(actor);
            }
        }
        if self.locomotion_dirty.is_empty() {
            return Ok(());
        }
        self.locomotion_refresh_scratch.clear();
        self.locomotion_refresh_scratch
            .extend(self.locomotion_dirty.iter().copied());
        for index in 0..self.locomotion_refresh_scratch.len() {
            let actor = self.locomotion_refresh_scratch[index];
            let active = self
                .world
                .body(actor)
                .ok()
                .and_then(|b| b.locomotion_projection())
                .is_some_and(|(_, drive)| {
                    drive.local_velocity != Vec3::ZERO || drive.angular_velocity != 0.
                });
            if !active || self.characters.get(actor).is_none() {
                self.locomotion_dirty.remove(&actor);
                continue;
            }
            if self.characters.reserved(actor)
                || self.inventory.reserved(actor)
                || self.npcs.reserved(actor)
            {
                continue;
            }
            match self.refresh_locomotion_actor(actor) {
                Ok(()) => {
                    self.locomotion_dirty.remove(&actor);
                }
                Err(bace_gameplay_api::locomotion::LocomotionRejection::Busy) => {}
                Err(_) => return Err(SimulationError::SkillRefresh),
            }
        }
        Ok(())
    }
}
