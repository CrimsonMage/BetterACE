//! GDLE SpellcastingManager single-projectile spawn at the pinned baseline.
//! Scalar authored frames and accepted velocities only; no client pose inputs.
use crate::{ProjectileLayoutError as E, ProjectileShape, ProjectileSpec, projectile_velocity};
use bace_geometry::Vec3;
#[derive(Clone, Copy, Debug)]
pub struct ProjectileActorFrame {
    pub position: Vec3,
    pub height: f32,
    pub radius: f32,
    pub heading: f32,
    pub velocity: Vec3,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileLaunch {
    pub position: Vec3,
    pub velocity: Vec3,
}
pub(super) fn valid(a: ProjectileActorFrame) -> bool {
    a.position.is_finite()
        && a.velocity.is_finite()
        && a.height.is_finite()
        && a.height >= 0.0
        && a.radius.is_finite()
        && a.radius >= 0.0
        && a.heading.is_finite()
}
pub(super) fn unit(v: Vec3) -> Option<Vec3> {
    let length = v.length_squared().sqrt();
    (length >= 0.0002 && length.is_finite()).then(|| v * (1.0 / length))
}
/// Frame::set_vector_heading, Euler order0, Quaternion::normalize, Frame::cache.
/// Preserve source f32 intermediates rather than substitute an orthogonal basis.
pub(super) fn heading_frame(direction: Vec3, local: Vec3) -> Vec3 {
    let d = unit(direction).expect("validated normalized heading");
    let z = (-((450.0 - f64::from(d.y).atan2(f64::from(d.x)).to_degrees()) % 360.0)).to_radians()
        as f32;
    let x = f64::from(d.z.clamp(-1.0, 1.0)).asin() as f32;
    let (si, ci) = (x * 0.5).sin_cos();
    let (sh, ch) = (z * 0.5).sin_cos();
    let mut q = [ci * ch, si * ch, si * sh, ci * sh];
    let reciprocal = 1.0 / (q[1] * q[1] + q[2] * q[2] + q[3] * q[3] + q[0] * q[0]).sqrt();
    for c in &mut q {
        *c *= reciprocal;
    }
    let [w, x, y, z] = q;
    let right = Vec3::new(
        (1.0 - y * (y * 2.0)) - z * (z * 2.0),
        x * (y * 2.0) + w * (z * 2.0),
        x * (z * 2.0) - w * (y * 2.0),
    );
    let forward = Vec3::new(
        x * (y * 2.0) - w * (z * 2.0),
        (1.0 - x * (x * 2.0)) - z * (z * 2.0),
        y * (z * 2.0) + w * (x * 2.0),
    );
    let up = Vec3::new(
        x * (z * 2.0) + w * (y * 2.0),
        y * (z * 2.0) - w * (x * 2.0),
        (1.0 - x * (x * 2.0)) - y * (y * 2.0),
    );
    right * local.x + forward * local.y + up * local.z
}
pub fn gdle_single_projectile(
    spec: &ProjectileSpec,
    source: ProjectileActorFrame,
    target: ProjectileActorFrame,
    self_target: bool,
    draw: Vec3,
) -> Result<ProjectileLaunch, E> {
    if !matches!(
        spec.shape,
        ProjectileShape::Bolt | ProjectileShape::Streak | ProjectileShape::Arc
    ) || spec.count != 1
        || spec.dimensions != [1, 1, 1]
        || spec.spread_degrees != 0.0
        || !matches!(spec.gravity, 0.0 | 9.8)
    {
        return Err(E::InvalidSpec);
    }
    if !valid(source)
        || !valid(target)
        || !spec.radius.is_finite()
        || spec.radius <= 0.0
        || !spec.offset.is_finite()
        || !spec.padding.is_finite()
        || !spec.perturbation.is_finite()
    {
        return Err(E::InvalidGeometry);
    }
    if !draw.is_finite()
        || [draw.x, draw.y, draw.z]
            .iter()
            .any(|v| !(-1.0..=1.0).contains(v))
    {
        return Err(E::InvalidRandom);
    }
    let arc = spec.gravity != 0.0;
    let mut position = source.position
        + Vec3::new(
            0.0,
            0.0,
            (f64::from(source.height) * if arc { 1.0 } else { 2.0 / 3.0 }) as f32,
        );
    let aim = if self_target {
        source.position
            + Vec3::new(
                -source.heading.sin() * 1000.0,
                source.heading.cos() * 1000.0,
                (f64::from(source.height) * 2.0 / 3.0) as f32,
            )
    } else {
        target.position
            + Vec3::new(
                0.0,
                0.0,
                (f64::from(target.height) * if arc { 5.0 / 6.0 } else { 2.0 / 3.0 }) as f32,
            )
    };
    let direction = if let Some(direction) = unit(aim - position) {
        position = position + direction * ((source.radius + spec.radius) + 0.1);
        direction
    } else {
        Vec3::new(-source.heading.sin(), source.heading.cos(), 0.0)
    };
    // GDLE perturbation is a world-axis offset; createOffset is in the newly
    // aligned projectile frame. For one projectile group offset is exactly zero.
    position = position
        + Vec3::new(
            draw.x * spec.perturbation.x * spec.padding.x,
            draw.y * spec.perturbation.y * spec.padding.y,
            draw.z * spec.perturbation.z * spec.padding.z,
        );
    position = position + heading_frame(direction, spec.offset);
    let aim = if self_target {
        aim
    } else {
        target.position + Vec3::new(0.0, 0.0, (f64::from(target.height) * 2.0 / 3.0) as f32)
    };
    let velocity = projectile_velocity(
        aim - position,
        target.velocity,
        spec.speed,
        spec.tracking && !self_target,
        spec.gravity,
    )?;
    if !position.is_finite() {
        return Err(E::InvalidGeometry);
    }
    Ok(ProjectileLaunch { position, velocity })
}
