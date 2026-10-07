use crate::PhysicsError;
use bace_geometry::{Aabb, Vec3};

/// Deliberately synthetic test geometry. DAT/BSP collision remains a separate
/// unimplemented acceptance requirement and cannot be enabled by this scene.
#[derive(Debug, Clone)]
pub struct SyntheticScene {
    floor: f32,
    bounds: Aabb,
    obstacles: Vec<Aabb>,
}

impl SyntheticScene {
    pub fn new(floor: f32, bounds: Aabb, obstacles: Vec<Aabb>) -> Result<Self, PhysicsError> {
        if !floor.is_finite() {
            return Err(PhysicsError::InvalidState);
        }
        Ok(Self {
            floor,
            bounds,
            obstacles,
        })
    }
    pub fn valid_placement(&self, position: Vec3, radius: f32) -> bool {
        position.is_finite()
            && radius.is_finite()
            && radius > 0.0
            && self.bounds.contains(position)
            && position.z >= self.floor + radius
            && !self
                .obstacles
                .iter()
                .any(|o| o.intersects_sphere(position, radius))
    }
    pub(crate) fn move_body(
        &self,
        position: Vec3,
        velocity: Vec3,
        radius: f32,
        dt: f32,
    ) -> (Vec3, Vec3, bool) {
        let delta = velocity * dt;
        let hit = self
            .obstacles
            .iter()
            .filter_map(|o| o.sweep(position, delta, radius))
            .fold(1.0_f32, f32::min);
        let fraction = if hit < 1.0 {
            (hit - 0.0001).max(0.0)
        } else {
            hit
        };
        let mut next = position + delta * fraction;
        let mut velocity = if hit < 1.0 { Vec3::ZERO } else { velocity };
        if !self.bounds.contains(next) {
            next = position;
            velocity = Vec3::ZERO;
        }
        let grounded = next.z <= self.floor + radius;
        if grounded {
            next.z = self.floor + radius;
            velocity.z = 0.0;
        }
        (next, velocity, grounded)
    }
}
