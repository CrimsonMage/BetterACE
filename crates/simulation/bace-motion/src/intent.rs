use bace_geometry::Vec3;

#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub speed: f32,
    pub jump_impulse: f32,
}

impl Capabilities {
    pub fn validate(self) -> Result<Self, MotionError> {
        if !self.speed.is_finite()
            || !(0.0..=50.0).contains(&self.speed)
            || !self.jump_impulse.is_finite()
            || !(0.0..=50.0).contains(&self.jump_impulse)
        {
            return Err(MotionError::Capabilities);
        }
        Ok(self)
    }
}

/// Direction and jump intent only. Client velocity/pose cannot be supplied here.
#[derive(Debug, Clone, Copy)]
pub struct MotionIntent {
    direction: Vec3,
    jump: bool,
}

impl MotionIntent {
    pub fn new(direction: Vec3, jump: bool) -> Result<Self, MotionError> {
        if !direction.is_finite() {
            return Err(MotionError::NonFinite);
        }
        Ok(Self {
            direction: direction.horizontal_clamped(),
            jump,
        })
    }
    pub fn velocity(self, capabilities: Capabilities) -> Vec3 {
        self.direction * capabilities.speed
    }
    pub fn wants_jump(self) -> bool {
        self.jump
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MotionError {
    #[error("motion contains a non-finite value")]
    NonFinite,
    #[error("invalid authoritative capabilities")]
    Capabilities,
}
