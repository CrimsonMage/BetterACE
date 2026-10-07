use crate::Vec3;

/// Axis-aligned bounds for synthetic collision fixtures; not AC BSP geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Option<Self> {
        (min.is_finite() && max.is_finite() && min.x < max.x && min.y < max.y && min.z < max.z)
            .then_some(Self { min, max })
    }
    pub fn contains(self, point: Vec3) -> bool {
        (0..3).all(|i| {
            point.component(i) >= self.min.component(i)
                && point.component(i) <= self.max.component(i)
        })
    }
    pub fn intersects_sphere(self, center: Vec3, radius: f32) -> bool {
        let distance: f32 = (0..3)
            .map(|i| {
                let p = center.component(i);
                let nearest = p.clamp(self.min.component(i), self.max.component(i));
                (p - nearest).powi(2)
            })
            .sum();
        distance < radius * radius
    }
    /// Conservative swept-sphere test via Minkowski-expanded box. Corner
    /// expansion is deliberately conservative, not an authentic AC shape test.
    pub fn sweep(self, from: Vec3, delta: Vec3, radius: f32) -> Option<f32> {
        // Intersect the open interior of each slab. Merely touching a face
        // while moving away or parallel to it must not trap a valid body.
        let mut entry = f32::NEG_INFINITY;
        let mut exit = f32::INFINITY;
        for axis in 0..3 {
            let origin = from.component(axis);
            let movement = delta.component(axis);
            let low = self.min.component(axis) - radius;
            let high = self.max.component(axis) + radius;
            if movement == 0.0 {
                if origin <= low || origin >= high {
                    return None;
                }
            } else {
                let a = (low - origin) / movement;
                let b = (high - origin) / movement;
                entry = entry.max(a.min(b));
                exit = exit.min(a.max(b));
                if entry >= exit {
                    return None;
                }
            }
        }
        (exit > 0.0 && entry <= 1.0).then_some(entry.max(0.0))
    }
}
