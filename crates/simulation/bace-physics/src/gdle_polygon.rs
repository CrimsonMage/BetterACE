//! Scalar GDLE polygon contact primitives at 353cbab52ef7da2b7063bc3e3f008461d8531693.
//! Source/PhatSDK/Polygon.cpp make_plane, polygon_hits_sphere_slow_but_sure,
//! pos_hits_sphere. Preserve explicit f32/f64 intermediate conversions.
//! GDLEnhanced source attribution retained; AGPL-3.0-only.
use crate::gdle_bsp::{BspQueryError, CollisionSphere};
use bace_geometry::Vec3;
pub(crate) const EPSILON: f32 = 0.0002;
pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}
pub(crate) fn bounded(v: Vec3) -> bool {
    v.is_finite()
        && v.x.abs() <= 1_000_000.0
        && v.y.abs() <= 1_000_000.0
        && v.z.abs() <= 1_000_000.0
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionPlane {
    pub normal: Vec3,
    pub distance: f32,
}
impl CollisionPlane {
    pub(crate) fn dot(self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
    pub(crate) fn validate(self) -> Result<(), BspQueryError> {
        let length = self.normal.length_squared();
        if !bounded(self.normal)
            || !self.distance.is_finite()
            || self.distance.abs() > 1_000_000.0
            || !(0.999..=1.001).contains(&length)
        {
            return Err(BspQueryError::InvalidGeometry);
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct GdlePolygon {
    pub(crate) vertices: Vec<Vec3>,
    pub(crate) plane: CollisionPlane,
}
impl GdlePolygon {
    /// One convex authored polygon in its original vertex order; no triangulation.
    /// Preparation computes its plane with GDLE's fan sum and averaged distance.
    pub fn prepare(vertices: Vec<Vec3>) -> Result<Self, BspQueryError> {
        if !(3..=255).contains(&vertices.len()) || vertices.iter().any(|v| !bounded(*v)) {
            return Err(BspQueryError::InvalidGeometry);
        }
        let mut normal = Vec3::ZERO;
        for i in 1..vertices.len() - 1 {
            normal = normal + cross(vertices[i] - vertices[0], vertices[i + 1] - vertices[0]);
        }
        let length = normal.length_squared().sqrt();
        if !length.is_finite() || length == 0.0 {
            return Err(BspQueryError::InvalidGeometry);
        }
        let factor = 1.0 / length;
        normal = normal * factor;
        let mut sum = 0.0f32;
        for v in &vertices {
            sum += normal.dot(*v);
        }
        let plane = CollisionPlane {
            normal,
            distance: -(sum / vertices.len() as f32),
        };
        plane.validate()?;
        // The upstream contact primitive assumes a planar convex authored polygon.
        // Refuse unsupported malformed inputs instead of trusting that assumption.
        for i in 0..vertices.len() {
            if plane.dot(vertices[i]).abs() > 0.005 {
                return Err(BspQueryError::InvalidGeometry);
            }
            let edge = vertices[(i + 1) % vertices.len()] - vertices[i];
            if edge.length_squared() == 0.0 {
                return Err(BspQueryError::InvalidGeometry);
            }
            let inward = cross(normal, edge);
            if vertices
                .iter()
                .any(|v| inward.dot(*v - vertices[i]) < -0.005)
            {
                return Err(BspQueryError::InvalidGeometry);
            }
        }
        Ok(Self { vertices, plane })
    }
    pub fn plane(&self) -> CollisionPlane {
        self.plane
    }
    /// Exact GDLE slow-but-sure overlap primitive. The contact lies on the polygon
    /// plane; it is not a fabricated time of impact or an accepted actor position.
    pub(crate) fn contact(&self, sphere: CollisionSphere) -> Option<Vec3> {
        let dp = f64::from(self.plane.dot(sphere.center));
        let radius = sphere.radius - EPSILON;
        if f64::from(radius) < dp.abs() {
            return None;
        }
        let circle = (f64::from(radius * radius) - dp * dp) as f32;
        let point = sphere.center - self.plane.normal * (dp as f32);
        let mut previous = self.vertices.len() - 1;
        for i in 0..self.vertices.len() {
            let last = self.vertices[previous];
            previous = i;
            let current = self.vertices[i];
            let edge = current - last;
            let inward = cross(self.plane.normal, edge);
            let distance = f64::from((point - last).dot(inward));
            if distance < 0.0 {
                let mut previous = self.vertices.len() - 1;
                for i in 0..self.vertices.len() {
                    let last = self.vertices[previous];
                    previous = i;
                    let current = self.vertices[i];
                    let edge = current - last;
                    let delta = point - last;
                    let inward = cross(self.plane.normal, edge);
                    let distance = f64::from(delta.dot(inward));
                    if distance < 0.0 {
                        if f64::from(inward.dot(inward) * circle) < distance * distance {
                            return None;
                        }
                        let along = f64::from(delta.dot(edge));
                        if along >= 0.0 && along <= f64::from(edge.dot(edge)) {
                            return Some(point);
                        }
                    }
                    if delta.dot(delta) <= circle {
                        return Some(point);
                    }
                }
                return None;
            }
        }
        Some(point)
    }
}
