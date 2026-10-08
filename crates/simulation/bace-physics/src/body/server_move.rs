//! Server approach control has its own sequence domain. It can request bounded
//! horizontal movement, never placement, contact, velocity or a jump.
use super::*;

impl Body {
    fn validate_server_move_speed(&self, speed: f32) -> Result<(), PhysicsError> {
        if !speed.is_finite()
            || speed <= 0.0
            || speed > 20.0
            || self.capabilities.speed * speed > 50.0
        {
            return Err(PhysicsError::InvalidState);
        }
        Ok(())
    }
    pub fn begin_server_move_scaled(
        &mut self,
        epoch: u16,
        control: TurnControl,
        intent: MotionIntent,
        speed: f32,
    ) -> Result<(), PhysicsError> {
        self.validate_server_move_speed(speed)?;
        self.begin_server_move(epoch, control, intent)?;
        self.server_move_speed = speed;
        Ok(())
    }
    pub fn continue_server_move_scaled(
        &mut self,
        epoch: u16,
        control: TurnControl,
        intent: MotionIntent,
        speed: f32,
    ) -> Result<(), PhysicsError> {
        self.validate_server_move_speed(speed)?;
        self.continue_server_move(epoch, control, intent)?;
        self.server_move_speed = speed;
        Ok(())
    }
    /// Allocate from the one body owner's controller sequence, including tokens
    /// issued but not yet started. Gameplay operation IDs are separate domains.
    pub fn next_server_control(&mut self) -> Result<TurnControl, PhysicsError> {
        let previous = [
            self.last_server_turn,
            self.last_server_move,
            self.last_issued_control,
        ]
        .into_iter()
        .flatten()
        .max();
        let next = match previous {
            None => TurnControl {
                owner: 1,
                sequence: 1,
            },
            Some(old) => match old.sequence.checked_add(1) {
                Some(sequence) => TurnControl { sequence, ..old },
                None => TurnControl {
                    owner: old.owner.checked_add(1).ok_or(PhysicsError::InvalidState)?,
                    sequence: 1,
                },
            },
        };
        self.last_issued_control = Some(next);
        Ok(next)
    }
    pub fn server_move(&self) -> Option<TurnControl> {
        self.server_move.map(|(control, _)| control)
    }
    pub fn begin_server_move(
        &mut self,
        epoch: u16,
        control: TurnControl,
        intent: MotionIntent,
    ) -> Result<(), PhysicsError> {
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if intent.wants_jump() {
            return Err(PhysicsError::InvalidState);
        }
        if control.owner == 0
            || control.sequence == 0
            || self.last_server_move.is_some_and(|last| control <= last)
        {
            return Err(PhysicsError::StaleSequence);
        }
        self.last_server_move = Some(control);
        self.server_move = Some((control, intent));
        self.server_move_speed = 1.0;
        // The new owner supersedes the preceding locomotion request. Finishing
        // an approach must not resume stale walking or a previously queued jump.
        self.intent = MotionIntent::new(Vec3::ZERO, false)?;
        self.locomotion = None;
        self.locomotion_controls = None;
        self.root_cursor = Default::default();
        self.jump_pending = false;
        self.authorized_jump = None;
        Ok(())
    }
    pub fn continue_server_move(
        &mut self,
        epoch: u16,
        control: TurnControl,
        intent: MotionIntent,
    ) -> Result<(), PhysicsError> {
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if intent.wants_jump() {
            return Err(PhysicsError::InvalidState);
        }
        if self.server_move() != Some(control) {
            return Err(PhysicsError::StaleSequence);
        }
        self.server_move = Some((control, intent));
        Ok(())
    }
    pub fn finish_server_move(&mut self, control: TurnControl) {
        if self.server_move() == Some(control) {
            self.server_move = None;
        }
    }
}
