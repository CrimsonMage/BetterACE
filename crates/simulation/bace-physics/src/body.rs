use crate::SyntheticScene;
use bace_geometry::Vec3;
use bace_motion::{Capabilities, MotionError, MotionIntent};

pub const STEP_SECONDS: f32 = 1.0 / 30.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcceptedState {
    position: Vec3,
    velocity: Vec3,
    grounded: bool,
    epoch: u16,
}

impl AcceptedState {
    pub fn position(self) -> Vec3 {
        self.position
    }
    pub fn velocity(self) -> Vec3 {
        self.velocity
    }
    pub fn grounded(self) -> bool {
        self.grounded
    }
    pub fn epoch(self) -> u16 {
        self.epoch
    }
}

pub struct Body {
    accepted: AcceptedState,
    capabilities: Capabilities,
    radius: f32,
    last_sequence: Option<u32>,
    intent: MotionIntent,
    jump_pending: bool,
}

impl Body {
    pub fn spawn(
        scene: &SyntheticScene,
        position: Vec3,
        radius: f32,
        capabilities: Capabilities,
    ) -> Result<Self, PhysicsError> {
        if !scene.valid_placement(position, radius) {
            return Err(PhysicsError::InvalidState);
        }
        let capabilities = capabilities.validate()?;
        let (position, velocity, grounded) = scene.move_body(position, Vec3::ZERO, radius, 0.0);
        Ok(Self {
            accepted: AcceptedState {
                position,
                velocity,
                grounded,
                epoch: 0,
            },
            capabilities,
            radius,
            last_sequence: None,
            intent: MotionIntent::new(Vec3::ZERO, false)?,
            jump_pending: false,
        })
    }
    pub fn accepted(&self) -> AcceptedState {
        self.accepted
    }
    /// Revalidate placement when transferring ownership to another scene.
    /// Constructing a body in one scene cannot authorize entry into another.
    pub fn validate_placement(&self, scene: &SyntheticScene) -> Result<(), PhysicsError> {
        if scene.valid_placement(self.accepted.position, self.radius) {
            Ok(())
        } else {
            Err(PhysicsError::InvalidState)
        }
    }
    pub fn submit_intent(
        &mut self,
        epoch: u16,
        sequence: u32,
        intent: MotionIntent,
    ) -> Result<(), PhysicsError> {
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if let Some(last) = self.last_sequence {
            let advance = sequence.wrapping_sub(last);
            if advance == 0 || advance >= 0x8000_0000 {
                return Err(PhysicsError::StaleSequence);
            }
        }
        self.last_sequence = Some(sequence);
        self.jump_pending = intent.wants_jump();
        self.intent = intent;
        Ok(())
    }
    pub fn step(&mut self, scene: &SyntheticScene) {
        let mut velocity = self.intent.velocity(self.capabilities);
        velocity.z = self.accepted.velocity.z;
        if self.jump_pending && self.accepted.grounded {
            velocity.z = self.capabilities.jump_impulse;
        }
        self.jump_pending = false;
        velocity.z -= 9.8 * STEP_SECONDS;
        let (position, velocity, grounded) =
            scene.move_body(self.accepted.position, velocity, self.radius, STEP_SECONDS);
        self.accepted = AcceptedState {
            position,
            velocity,
            grounded,
            epoch: self.accepted.epoch,
        };
    }
    /// Server-only privileged relocation, never a decoded client position path.
    pub fn server_teleport(
        &mut self,
        scene: &SyntheticScene,
        position: Vec3,
    ) -> Result<(), PhysicsError> {
        if !scene.valid_placement(position, self.radius) {
            return Err(PhysicsError::InvalidState);
        }
        let (position, velocity, grounded) =
            scene.move_body(position, Vec3::ZERO, self.radius, 0.0);
        self.accepted = AcceptedState {
            position,
            velocity,
            grounded,
            epoch: self.accepted.epoch.wrapping_add(1),
        };
        self.last_sequence = None;
        self.intent = MotionIntent::new(Vec3::ZERO, false)?;
        self.jump_pending = false;
        Ok(())
    }
    /// Observation cannot mutate authority. Full history-based reconciliation is
    /// deferred until AC motion/DAT paths are implemented and client-tested.
    pub fn observe(&self, epoch: u16, position: Vec3) -> Result<AcceptedState, PhysicsError> {
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if !position.is_finite() {
            return Err(PhysicsError::InvalidState);
        }
        Ok(self.accepted)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PhysicsError {
    #[error("invalid physical state or unavailable placement")]
    InvalidState,
    #[error("stale movement epoch")]
    StaleEpoch,
    #[error("duplicate or stale movement sequence")]
    StaleSequence,
    #[error(transparent)]
    Motion(#[from] MotionError),
}
