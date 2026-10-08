//! GDLE 353cbab SpellcastingManager::LaunchProjectileSpell and spawn helpers.
//! Original loop order, ring offsets and frame arithmetic; bounded publication.
use crate::launch::{heading_frame, unit, valid};
use crate::{
    ProjectileActorFrame, ProjectileLaunch, ProjectileLayoutError as E, ProjectileShape,
    ProjectileSpec, projectile_velocity,
};
use bace_geometry::Vec3;

fn rotate(v: Vec3, heading: f32) -> Vec3 {
    let (s, c) = heading.sin_cos();
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}

// This deliberately retains GDLE's sequential assignment, including using the
// updated x in y. It is source behavior for non-self angled casts, not an ideal
// rotation and not independently established retail behavior.
fn source_angle(mut v: Vec3, angle: f64) -> Vec3 {
    let (s, c) = angle.sin_cos();
    v.x = (f64::from(v.x) * c - f64::from(v.y) * s) as f32;
    v.y = (f64::from(v.x) * s + f64::from(v.y) * c) as f32;
    v
}

/// Calculate a complete bounded GDLE group before world publication. Untargeted
/// outward spells pass the source as target with `self_target = true`.
pub fn gdle_projectiles(
    spec: &ProjectileSpec,
    source: ProjectileActorFrame,
    target: ProjectileActorFrame,
    self_target: bool,
    draws: &[Vec3],
) -> Result<Vec<ProjectileLaunch>, E> {
    let [nx, ny, nz] = spec.dimensions;
    if spec.shape == ProjectileShape::Strike
        || spec.count == 0
        || spec.count > 1024
        || [nx, ny, nz].contains(&0)
        || u64::from(nx) * u64::from(ny) * u64::from(nz) != u64::from(spec.count)
        || !spec.spread_degrees.is_finite()
        || !(0.0..=360.0).contains(&spec.spread_degrees)
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
    let random = spec.perturbation != Vec3::ZERO;
    if random
        && (draws.len() != usize::from(spec.count)
            || draws.iter().any(|d| {
                !d.is_finite() || [d.x, d.y, d.z].iter().any(|v| !(-1.0..=1.0).contains(v))
            }))
    {
        return Err(E::InvalidRandom);
    }
    let angled = spec.spread_degrees > 0.0 && nx > 1;
    let ring = angled && spec.spread_degrees > 180.0;
    let arc = spec.gravity != 0.0;
    let mut result = Vec::with_capacity(usize::from(spec.count));
    for x in 0..nx {
        for y in 0..ny {
            for z in 0..nz {
                let theta = if angled {
                    ((f64::from(x) / f64::from(nx) - (f64::from(nx) - 1.0) / (2.0 * f64::from(nx)))
                        * f64::from(spec.spread_degrees))
                    .to_radians()
                } else {
                    0.0
                };
                let mut position = source.position
                    + Vec3::new(
                        0.0,
                        0.0,
                        (f64::from(source.height) * if arc { 1.0 } else { 2.0 / 3.0 }) as f32,
                    );
                let self_aim = {
                    let (s, c) = theta.sin_cos();
                    let mut height = (f64::from(source.height) * (2.0 / 3.0)) as f32;
                    if ring {
                        height = (f64::from(height) * 1.5) as f32;
                    }
                    source.position
                        + rotate(
                            Vec3::new((-1000.0 * s) as f32, (1000.0 * c) as f32, height),
                            source.heading,
                        )
                };
                let offset = if self_target {
                    self_aim - position
                } else {
                    source_angle(
                        target.position
                            + Vec3::new(
                                0.0,
                                0.0,
                                (f64::from(target.height) * if arc { 5.0 / 6.0 } else { 2.0 / 3.0 })
                                    as f32,
                            )
                            - position,
                        theta,
                    )
                };
                let direction = if let Some(direction) = unit(offset) {
                    let distance = f64::from(source.radius + spec.radius)
                        + f64::from(0.1_f32)
                        + if ring { 0.5 } else { 0.0 };
                    position = position + direction * distance as f32;
                    direction
                } else {
                    rotate(Vec3::new(0.0, 1.0, 0.0), source.heading)
                };
                if ring {
                    position = position + rotate(Vec3::new(0.0, 0.1, 0.0), source.heading);
                }
                if random {
                    let d = draws[result.len()];
                    position = position
                        + Vec3::new(
                            d.x * spec.perturbation.x * spec.padding.x,
                            d.y * spec.perturbation.y * spec.padding.y,
                            d.z * spec.perturbation.z * spec.padding.z,
                        );
                }
                position = position + heading_frame(direction, spec.offset);
                let velocity_offset = if self_target {
                    self_aim - position
                } else {
                    source_angle(
                        target.position
                            + Vec3::new(0.0, 0.0, (f64::from(target.height) * (2.0 / 3.0)) as f32)
                            - position,
                        theta,
                    )
                };
                let velocity = projectile_velocity(
                    velocity_offset,
                    target.velocity,
                    spec.speed,
                    spec.tracking && !self_target,
                    spec.gravity,
                )?;
                if !angled {
                    // C++ compound assignment promotes the centering arithmetic to f64.
                    let axis = |i: u16, n: u16, p: f32| {
                        (f64::from(f32::from(i) * p) - f64::from(p) * ((f64::from(n) - 1.0) / 2.0))
                            as f32
                    };
                    position = position
                        + heading_frame(
                            direction,
                            Vec3::new(
                                axis(x, nx, spec.padding.x),
                                axis(y, ny, spec.padding.y),
                                axis(z, nz, spec.padding.z),
                            ),
                        );
                }
                if !position.is_finite() {
                    return Err(E::InvalidGeometry);
                }
                result.push(ProjectileLaunch { position, velocity });
            }
        }
    }
    Ok(result)
}
