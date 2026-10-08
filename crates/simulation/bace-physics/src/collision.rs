use crate::PhysicsError;
use bace_geometry::{Aabb, Vec3};

/// Deliberately synthetic test geometry. DAT/BSP collision remains a separate
/// unimplemented acceptance requirement and cannot be enabled by this scene.
#[derive(Debug, Clone)]
pub struct SyntheticScene {
    floor: f32,
    bounds: Aabb,
    obstacles: Vec<Aabb>,
    dynamic: Vec<(u32, Aabb, bool)>,
}

impl SyntheticScene {
    pub fn swept_fraction(
        &self,
        from: Vec3,
        to: Vec3,
        radius: f32,
    ) -> Result<Option<f32>, PhysicsError> {
        if !from.is_finite()
            || !to.is_finite()
            || !radius.is_finite()
            || radius <= 0.0
            || !self.bounds.contains(from)
            || !self.bounds.contains(to)
        {
            return Err(PhysicsError::InvalidState);
        }
        let mut hit = self
            .obstacles()
            .filter_map(|obstacle| obstacle.sweep(from, to - from, radius))
            .min_by(f32::total_cmp);
        if to.z < self.floor + radius && to.z < from.z {
            let floor = ((from.z - self.floor - radius) / (from.z - to.z)).clamp(0.0, 1.0);
            hit = Some(hit.map_or(floor, |h| h.min(floor)));
        }
        Ok(hit)
    }
    pub fn new(floor: f32, bounds: Aabb, obstacles: Vec<Aabb>) -> Result<Self, PhysicsError> {
        if !floor.is_finite() {
            return Err(PhysicsError::InvalidState);
        }
        Ok(Self {
            floor,
            bounds,
            obstacles,
            dynamic: Vec::new(),
        })
    }
    pub fn register_dynamic_obstacle(
        &mut self,
        id: u32,
        bounds: Aabb,
        solid: bool,
    ) -> Result<(), PhysicsError> {
        if id == 0
            || self.dynamic.len() >= 4096
            || self.dynamic.iter().any(|(existing, _, _)| *existing == id)
        {
            return Err(PhysicsError::InvalidState);
        }
        self.dynamic.push((id, bounds, solid));
        Ok(())
    }
    pub fn set_dynamic_solid(&mut self, id: u32, solid: bool) -> Result<(), PhysicsError> {
        let entry = self
            .dynamic
            .iter_mut()
            .find(|(existing, _, _)| *existing == id)
            .ok_or(PhysicsError::InvalidState)?;
        entry.2 = solid;
        Ok(())
    }
    fn obstacles(&self) -> impl Iterator<Item = &Aabb> {
        self.obstacles.iter().chain(
            self.dynamic
                .iter()
                .filter(|(_, _, solid)| *solid)
                .map(|(_, bounds, _)| bounds),
        )
    }
    pub(crate) fn dynamic_contact(&self, from: Vec3, delta: Vec3, radius: f32) -> Option<u32> {
        let static_hit = self
            .obstacles
            .iter()
            .filter_map(|bounds| bounds.sweep(from, delta, radius))
            .fold(1.0_f32, f32::min);
        self.dynamic
            .iter()
            .filter(|(_, _, solid)| *solid)
            .filter_map(|(id, bounds, _)| {
                bounds
                    .sweep(from, delta, radius)
                    .filter(|hit| *hit <= static_hit)
                    .map(|hit| (*id, hit))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(id, _)| id)
    }
    pub fn valid_placement(&self, position: Vec3, radius: f32) -> bool {
        position.is_finite()
            && radius.is_finite()
            && radius > 0.0
            && self.bounds.contains(position)
            && position.z >= self.floor + radius
            && !self
                .obstacles()
                .any(|o| o.intersects_sphere(position, radius))
    }
    /// Conservative synthetic line-of-sight query; not DAT/BSP qualification.
    pub fn segment_clear(&self, from: Vec3, to: Vec3) -> bool {
        self.segment_clear_ignoring(from, to, None)
    }
    pub fn segment_clear_ignoring(&self, from: Vec3, to: Vec3, ignore: Option<u32>) -> bool {
        from.is_finite()
            && to.is_finite()
            && self.bounds.contains(from)
            && self.bounds.contains(to)
            && from.z >= self.floor
            && to.z >= self.floor
            && !self
                .obstacles
                .iter()
                .chain(
                    self.dynamic
                        .iter()
                        .filter(|(id, _, solid)| *solid && Some(*id) != ignore)
                        .map(|(_, bounds, _)| bounds),
                )
                .any(|obstacle| obstacle.sweep(from, to - from, 0.0).is_some())
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
            .obstacles()
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
