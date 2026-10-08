//! ACE WorldObject_Magic projectile origin/spread calculation at the pin.
//! Local offsets only: the world/physics owner performs admitted frame conversion
//! and collision-gated launch. Supplied perturbations are explicit RNG samples.
use crate::{ProjectileShape, ProjectileSpec};
use bace_geometry::Vec3;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileLayoutError {
    InvalidGeometry,
    InvalidSpec,
    InvalidRandom,
    OutputCapacity,
}
fn rotate(v: Vec3, angle: f32) -> Vec3 {
    let (s, c) = angle.sin_cos();
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}
pub fn spread_angle_step(angle: f32, count: u16) -> Result<f32, ProjectileLayoutError> {
    if !angle.is_finite() || !(0.0..=360.0).contains(&angle) || count == 0 || count > 1024 {
        return Err(ProjectileLayoutError::InvalidSpec);
    }
    Ok(if angle == 0.0 || count == 1 {
        0.0
    } else {
        angle / f32::from(count - (count % 2))
    })
}
pub fn projectile_pre_offset(
    shape: ProjectileShape,
    caster_height: f32,
    caster_radius: f32,
    projectile_radius: f32,
    target: Option<(Vec3, f32)>,
) -> Result<Vec3, ProjectileLayoutError> {
    if [caster_height, caster_radius, projectile_radius]
        .iter()
        .any(|n| !n.is_finite() || *n < 0.0)
    {
        return Err(ProjectileLayoutError::InvalidGeometry);
    }
    let factor = if shape == ProjectileShape::Arc {
        1.0
    } else {
        2.0 / 3.0
    };
    let pre = Vec3::new(0.0, 0.0, caster_height * factor);
    let Some((offset, height)) = target else {
        return Ok(pre);
    };
    if !offset.is_finite() || !height.is_finite() || height < 0.0 {
        return Err(ProjectileLayoutError::InvalidGeometry);
    }
    let end = if shape == ProjectileShape::Arc {
        5.0 / 6.0
    } else {
        2.0 / 3.0
    };
    let global = offset + Vec3::new(0.0, 0.0, height * end - caster_height * factor);
    let local = rotate(global, global.x.atan2(global.y));
    let length = local.length_squared().sqrt();
    if !length.is_finite() || length == 0.0 {
        return Err(ProjectileLayoutError::InvalidGeometry);
    }
    let radius = caster_radius + projectile_radius;
    Ok(pre + local * (radius / length) - Vec3::new(0.0, radius, 0.0))
}
pub fn projectile_origins(
    spec: &ProjectileSpec,
    caster_radius: f32,
    pre_offset: Vec3,
    target_cylinder_distance: Option<f32>,
    perturbation: Vec3,
    draws: &[Vec3],
    out: &mut Vec<Vec3>,
) -> Result<(), ProjectileLayoutError> {
    let angle = spread_angle_step(spec.spread_degrees, spec.count)?;
    if !caster_radius.is_finite()
        || caster_radius < 0.0
        || !spec.radius.is_finite()
        || spec.radius <= 0.0
        || !pre_offset.is_finite()
        || !spec.offset.is_finite()
        || !spec.padding.is_finite()
        || !perturbation.is_finite()
        || target_cylinder_distance.is_some_and(|d| !d.is_finite())
    {
        return Err(ProjectileLayoutError::InvalidGeometry);
    }
    let dims = spec.dimensions;
    if dims.contains(&0)
        || dims.iter().any(|d| *d > 1024)
        || u64::from(dims[0]) * u64::from(dims[1]) * u64::from(dims[2]) < u64::from(spec.count)
    {
        return Err(ProjectileLayoutError::InvalidSpec);
    }
    let random = perturbation != Vec3::ZERO;
    if random
        && (draws.len() < usize::from(spec.count)
            || draws[..usize::from(spec.count)].iter().any(|v| {
                !v.is_finite() || [v.x, v.y, v.z].iter().any(|n| !(-1.0..=1.0).contains(n))
            }))
    {
        return Err(ProjectileLayoutError::InvalidRandom);
    }
    if out.capacity() - out.len() < usize::from(spec.count) {
        return Err(ProjectileLayoutError::OutputCapacity);
    }
    let mut radius = caster_radius * 2.0 + spec.radius * 2.0;
    if target_cylinder_distance.is_some_and(|d| d < 0.6) {
        radius = caster_radius + spec.radius;
    }
    if spec.spread_degrees == 360.0 {
        radius *= 0.6;
    }
    let base = spec.offset + pre_offset + Vec3::new(0.0, radius, 0.0);
    let mut index = 0;
    // Validate the complete generated set before publication; no partial output.
    let generate = |index: usize| -> Vec3 {
        let width = usize::from(dims[0]);
        let plane = width * usize::from(dims[1]);
        let x = index % width;
        let y = index / width % usize::from(dims[1]);
        let z = index / plane;
        let row_start = index - x;
        let odd = width.min(usize::from(spec.count) - row_start) % 2 == 1;
        let mut origin = base;
        if random {
            let d = draws[index];
            origin = origin
                + Vec3::new(
                    d.x * perturbation.x * spec.padding.x,
                    d.y * perturbation.y * spec.padding.y,
                    d.z * perturbation.z * spec.padding.z,
                );
        }
        if !odd && spec.spread_degrees == 0.0 {
            origin.x += spec.padding.x * 0.5 + spec.radius;
        }
        let factor = if spec.spread_degrees == 0.0 {
            if odd {
                (x as f32 * 0.5).ceil()
            } else {
                (x as f32 * 0.5).floor()
            }
        } else {
            0.0
        };
        origin = origin
            + Vec3::new(
                (spec.radius * 2.0 + spec.padding.x) * factor,
                (spec.radius * 2.0 + spec.padding.y) * y as f32,
                (spec.radius * 2.0 + spec.padding.z) * z as f32,
            );
        if spec.spread_degrees == 0.0 {
            if x % 2 == usize::from(odd) {
                origin.x *= -1.0;
            }
        } else {
            let steps = x.div_ceil(2) as f32 * if x.is_multiple_of(2) { -1.0 } else { 1.0 };
            origin = rotate(origin, (angle * steps).to_radians());
        }
        origin
    };
    while index < usize::from(spec.count) {
        if !generate(index).is_finite() {
            return Err(ProjectileLayoutError::InvalidGeometry);
        }
        index += 1;
    }
    for index in 0..usize::from(spec.count) {
        out.push(generate(index));
    }
    Ok(())
}

/// GDLE LaunchProjectileSpell: f32 product and C++ round (away from zero),
/// preserving one health. Stamina/mana may be exhausted completely.
pub fn life_projectile_drain(
    current: u32,
    proportion: f32,
    health: bool,
) -> Result<u32, ProjectileLayoutError> {
    if !proportion.is_finite() || !(0.0..=1.0).contains(&proportion) {
        return Err(ProjectileLayoutError::InvalidSpec);
    }
    let amount = (current as f32 * proportion).round() as u32;
    Ok(amount.min(if health {
        current.saturating_sub(1)
    } else {
        current
    }))
}
