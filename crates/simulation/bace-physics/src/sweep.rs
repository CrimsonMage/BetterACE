//! Continuous scalar sphere casts against convex authored polygons and spheres.
//! This collision primitive is independent of tick integration and never accepts
//! client poses. Every edge and vertex participates, including grazing contacts.
use crate::GdlePolygon;
use bace_geometry::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepContact {
    pub fraction: f32,
    pub normal: Vec3,
}
fn unit(v: Vec3) -> Option<Vec3> {
    let length = v.length_squared().sqrt();
    (length > 0.000001 && length.is_finite()).then(|| v * (1.0 / length))
}
fn quadratic(a: f32, b: f32, c: f32) -> Option<f32> {
    if a <= 0.0000000001 {
        return None;
    }
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let time = (-b - discriminant.sqrt()) / a;
    (0.0..=1.0).contains(&time).then_some(time)
}
pub fn sweep_spheres(
    from: Vec3,
    displacement: Vec3,
    radius: f32,
    center: Vec3,
    other_radius: f32,
) -> Option<SweepContact> {
    let relative = from - center;
    let reach = radius + other_radius;
    let fraction = quadratic(
        displacement.dot(displacement),
        relative.dot(displacement),
        relative.dot(relative) - reach * reach,
    )?;
    let normal = unit(relative + displacement * fraction)?;
    (displacement.dot(normal) < -0.000001).then_some(SweepContact { fraction, normal })
}
fn nearer(best: &mut Option<SweepContact>, hit: Option<SweepContact>) {
    if let Some(hit) = hit
        && best.is_none_or(|old| hit.fraction < old.fraction)
    {
        *best = Some(hit);
    }
}
/// Expanded vertical-cylinder intersection. Side and top/bottom planes retain
/// distinct contact normals as in ACE CylSphere's collision response.
pub(crate) fn sweep_cylinder(
    from: Vec3,
    delta: Vec3,
    radius: f32,
    base: Vec3,
    other_radius: f32,
    height: f32,
) -> Option<SweepContact> {
    let p = from - base;
    let reach = radius + other_radius;
    let mut best = None;
    if let Some(fraction) = quadratic(
        delta.x * delta.x + delta.y * delta.y,
        p.x * delta.x + p.y * delta.y,
        p.x * p.x + p.y * p.y - reach * reach,
    ) {
        let z = p.z + delta.z * fraction;
        if z >= -radius
            && z <= height + radius
            && let Some(normal) = unit(Vec3::new(
                p.x + delta.x * fraction,
                p.y + delta.y * fraction,
                0.0,
            ))
        {
            best = Some(SweepContact { fraction, normal });
        }
    }
    if delta.z != 0.0 {
        let (plane, normal) = if delta.z < 0.0 {
            (height + radius, Vec3::new(0.0, 0.0, 1.0))
        } else {
            (-radius, Vec3::new(0.0, 0.0, -1.0))
        };
        let fraction = (plane - p.z) / delta.z;
        let point = p + delta * fraction;
        if (0.0..=1.0).contains(&fraction) && point.x * point.x + point.y * point.y <= reach * reach
        {
            nearer(&mut best, Some(SweepContact { fraction, normal }));
        }
    }
    best
}
impl GdlePolygon {
    pub fn vertices(&self) -> &[Vec3] {
        &self.vertices
    }
    /// `two_sided=false` preserves an authored outward face's collision side.
    pub fn sweep_sphere(
        &self,
        from: Vec3,
        displacement: Vec3,
        radius: f32,
        two_sided: bool,
    ) -> Option<SweepContact> {
        let distance = self.plane.normal.dot(from) + self.plane.distance;
        let normal = if two_sided && distance < 0.0 {
            self.plane.normal * -1.0
        } else {
            self.plane.normal
        };
        if !two_sided && distance < -0.0002 {
            return None;
        }
        let velocity = normal.dot(displacement);
        let mut best = None;
        if velocity < -0.000001 {
            let signed = if two_sided { distance.abs() } else { distance };
            let fraction = ((radius - signed) / velocity).max(0.0);
            if fraction <= 1.0 && signed >= radius - 0.0002 {
                let point = from + displacement * fraction - normal * radius;
                if self.contains_projection(point, 0.0002) {
                    best = Some(SweepContact { fraction, normal });
                }
            }
        }
        for i in 0..self.vertices.len() {
            let a = self.vertices[i];
            let b = self.vertices[(i + 1) % self.vertices.len()];
            nearer(&mut best, sweep_spheres(from, displacement, radius, a, 0.0));
            let edge = b - a;
            let length = edge.dot(edge);
            let relative = from - a;
            let projected_velocity = displacement - edge * (displacement.dot(edge) / length);
            let projected_relative = relative - edge * (relative.dot(edge) / length);
            if let Some(fraction) = quadratic(
                projected_velocity.dot(projected_velocity),
                projected_relative.dot(projected_velocity),
                projected_relative.dot(projected_relative) - radius * radius,
            ) {
                let center = from + displacement * fraction;
                let along = (center - a).dot(edge) / length;
                if (0.0..=1.0).contains(&along)
                    && let Some(normal) = unit(center - (a + edge * along))
                    && displacement.dot(normal) < -0.000001
                {
                    nearer(&mut best, Some(SweepContact { fraction, normal }));
                }
            }
        }
        best
    }
    pub fn contains_projection(&self, point: Vec3, tolerance: f32) -> bool {
        (0..self.vertices.len()).all(|i| {
            let a = self.vertices[i];
            let edge = self.vertices[(i + 1) % self.vertices.len()] - a;
            crate::gdle_polygon::cross(self.plane.normal, edge).dot(point - a) >= -tolerance
        })
    }
}
