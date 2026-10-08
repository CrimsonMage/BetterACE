//! Bounded server approach intents share the existing authoritative body.
use super::*;
use bace_motion::{MotionIntent, TurnControl};
impl World {
    pub fn begin_server_move_scaled(
        &mut self,
        actor: EntityId,
        epoch: u16,
        control: TurnControl,
        intent: MotionIntent,
        speed: f32,
    ) -> Result<(), WorldError> {
        self.validate_server_move(actor)?;
        self.body_mut(actor)?
            .begin_server_move_scaled(epoch, control, intent, speed)?;
        Ok(())
    }
    pub fn continue_server_move_scaled(
        &mut self,
        actor: EntityId,
        epoch: u16,
        control: TurnControl,
        intent: MotionIntent,
        speed: f32,
    ) -> Result<(), WorldError> {
        self.validate_server_move(actor)?;
        self.body_mut(actor)?
            .continue_server_move_scaled(epoch, control, intent, speed)?;
        Ok(())
    }
    pub fn next_server_control(&mut self, actor: EntityId) -> Result<TurnControl, WorldError> {
        Ok(self.body_mut(actor)?.next_server_control()?)
    }
    fn validate_server_move(&self, actor: EntityId) -> Result<(), WorldError> {
        let owned = self.actors.get(&actor).ok_or(WorldError::MissingActor)?;
        if self.actor_region_dormant(actor) || self.geometry_blocked.contains_key(&actor) {
            return Err(WorldError::MissingGeometry);
        }
        if owned.body.collision_shape().is_some() {
            if self.geometry.is_none() {
                return Err(WorldError::MissingGeometry);
            }
        } else if !self.scenes.contains_key(&owned.cell) {
            return Err(WorldError::MissingGeometry);
        }
        Ok(())
    }
    pub fn begin_server_move(
        &mut self,
        actor: EntityId,
        epoch: u16,
        control: TurnControl,
        intent: MotionIntent,
    ) -> Result<(), WorldError> {
        self.validate_server_move(actor)?;
        self.body_mut(actor)?
            .begin_server_move(epoch, control, intent)?;
        Ok(())
    }
    pub fn continue_server_move(
        &mut self,
        actor: EntityId,
        epoch: u16,
        control: TurnControl,
        intent: MotionIntent,
    ) -> Result<(), WorldError> {
        self.validate_server_move(actor)?;
        self.body_mut(actor)?
            .continue_server_move(epoch, control, intent)?;
        Ok(())
    }
    pub fn finish_server_move(&mut self, actor: EntityId, control: TurnControl) {
        if let Some(owned) = self.actors.get_mut(&actor) {
            owned.body.finish_server_move(control);
        }
    }
}
