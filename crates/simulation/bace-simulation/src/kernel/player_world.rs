use super::*;
use crate::player_world::{PlayerWorldSaveMarker, PlayerWorldSnapshot, WorldDirty};
impl Kernel {
    pub(super) fn needs_world_state_transfer(&self, actor: EntityId) -> bool {
        self.player_world_dirty.contains_key(&actor)
            || match self.player_world_snapshot(actor) {
                Ok(current) => self.player_world_seen.get(&actor) != Some(&current),
                Err(_) => true,
            }
    }
    pub fn player_world_snapshot(
        &self,
        actor: EntityId,
    ) -> Result<PlayerWorldSnapshot, WorldError> {
        let (cell, state) = self.world.actor_state(actor)?;
        Ok(PlayerWorldSnapshot {
            cell,
            position: state.position(),
            heading: state.heading_radians(),
            vitals: [
                bace_entity::EntityVital::Health,
                bace_entity::EntityVital::Stamina,
                bace_entity::EntityVital::Mana,
            ]
            .map(|v| self.world.vital(actor, v).ok()),
        })
    }
    pub(super) fn sync_player_world(&mut self) -> Result<(), SimulationError> {
        self.player_world_scratch.clear();
        self.player_world_scratch.extend(
            self.world
                .states()
                .filter_map(|(id, _, _)| self.characters.get(id).is_some().then_some(id)),
        );
        for index in 0..self.player_world_scratch.len() {
            let actor = self.player_world_scratch[index];
            let snapshot = self.player_world_snapshot(actor)?;
            let was_exhausted = self
                .player_world_seen
                .get(&actor)
                .and_then(|previous| previous.vitals[1])
                .is_some_and(|stamina| stamina.current == 0);
            let exhausted = snapshot.vitals[1].is_some_and(|stamina| stamina.current == 0);
            if was_exhausted != exhausted {
                self.locomotion_dirty.insert(actor);
            }
            let changed = self.player_world_seen.get(&actor) != Some(&snapshot);
            if changed {
                self.player_world_seen.insert(actor, snapshot);
                let dirty = self.player_world_dirty.entry(actor).or_insert(WorldDirty {
                    first_tick: self.tick,
                    needs_revision: false,
                    in_flight: None,
                });
                dirty.needs_revision = true;
                if let Some((marker, after)) = &mut dirty.in_flight
                    && marker.snapshot != snapshot
                    && after.is_none()
                {
                    *after = Some(self.tick);
                }
            }
            if self
                .player_world_dirty
                .get(&actor)
                .is_some_and(|d| d.needs_revision)
                && !self.npcs.reserved(actor)
                && !self.inventory.reserved(actor)
                && !self.housing.reserved(actor)
                && !self.pets.reserved(actor)
                && !self.portals.reserved(actor)
                && !self.characters.reserved(actor)
                && self
                    .characters
                    .touch_auxiliary(actor)
                    .map_err(|_| SimulationError::AuxiliaryRevision)?
            {
                self.player_world_dirty
                    .get_mut(&actor)
                    .expect("selected dirty actor")
                    .needs_revision = false;
            }
        }
        Ok(())
    }
    /// Dirty age is retained through reservations and failed writes.
    pub fn player_world_dirty_since(&self, actor: EntityId) -> Option<u64> {
        self.player_world_dirty.get(&actor).map(|d| d.first_tick)
    }
    pub fn mark_player_world_save(
        &mut self,
        marker: PlayerWorldSaveMarker,
    ) -> Result<(), SimulationError> {
        if self
            .characters
            .get(marker.actor)
            .is_none_or(|c| c.revision() != marker.revision)
            || self.player_world_snapshot(marker.actor)? != marker.snapshot
            || self
                .player_world_dirty
                .get(&marker.actor)
                .is_some_and(|d| d.needs_revision)
        {
            return Err(SimulationError::AuxiliaryRevision);
        }
        let dirty = self
            .player_world_dirty
            .entry(marker.actor)
            .or_insert(WorldDirty {
                first_tick: self.tick,
                needs_revision: false,
                in_flight: None,
            });
        if dirty.in_flight.is_some() {
            return Err(SimulationError::AuxiliaryRevision);
        }
        dirty.in_flight = Some((marker, None));
        Ok(())
    }
    pub fn acknowledge_player_world_save(
        &mut self,
        marker: PlayerWorldSaveMarker,
    ) -> Result<(), SimulationError> {
        let current = self.player_world_snapshot(marker.actor)?;
        let dirty = self
            .player_world_dirty
            .get_mut(&marker.actor)
            .ok_or(SimulationError::AuxiliaryRevision)?;
        let Some((expected, after)) = dirty.in_flight else {
            return Err(SimulationError::AuxiliaryRevision);
        };
        if expected != marker {
            return Err(SimulationError::AuxiliaryRevision);
        }
        if current == marker.snapshot {
            self.player_world_dirty.remove(&marker.actor);
            self.player_world_seen.insert(marker.actor, current);
        } else {
            dirty.first_tick = after.unwrap_or(dirty.first_tick);
            dirty.in_flight = None;
        }
        Ok(())
    }
    pub fn fail_player_world_save(
        &mut self,
        marker: PlayerWorldSaveMarker,
    ) -> Result<(), SimulationError> {
        let dirty = self
            .player_world_dirty
            .get_mut(&marker.actor)
            .ok_or(SimulationError::AuxiliaryRevision)?;
        if dirty
            .in_flight
            .is_none_or(|(expected, _)| expected != marker)
        {
            return Err(SimulationError::AuxiliaryRevision);
        }
        dirty.in_flight = None;
        Ok(())
    }
}

#[cfg(test)]
#[path = "player_world_tests.rs"]
mod tests;
