//! Loading bodies settle against verified geometry but are not targetable or
//! dynamic colliders until their trusted entered transition completes.
use super::*;
impl World {
    pub fn validate_player_entry(&self, actor: EntityId) -> Result<(), WorldError> {
        if actor.0 == 0 || self.entry_pending.contains(&actor) || self.entry_pending.len() >= 4096 {
            return Err(WorldError::InvalidMotion);
        }
        Ok(())
    }
    pub fn begin_player_entry(&mut self, actor: EntityId) -> Result<(), WorldError> {
        self.validate_player_entry(actor)?;
        if self.combatant(actor).is_none_or(|c| !c.profile().player)
            || !self.actors.contains_key(&actor)
        {
            return Err(WorldError::MissingActor);
        }
        self.entry_pending.insert(actor);
        self.visibility.invalidate();
        Ok(())
    }
    pub fn finish_player_entry(&mut self, actor: EntityId) {
        if self.entry_pending.remove(&actor) {
            self.visibility.invalidate();
        }
    }
}
