//! Reviewed ACE Strike gap-fill at 47edade3: target-relative launch and the
//! default lateral trajectory solver. GDLE has no strike-specific placement.
//! Trajectory quadratic provenance: ACE / Graphics Gems I, Jochen Schwarze.
use crate::launch::{unit, valid};
use crate::{
    ProjectileActorFrame, ProjectileLaunch, ProjectileLayoutError as E, ProjectileShape,
    ProjectileSpec, projectile_origins, projectile_pre_offset,
};
use bace_geometry::Vec3;

fn rotate(v: Vec3, heading: f32) -> Vec3 {
    // System.Numerics Vector3.Transform around Z uses a quaternion, preserving
    // its f32 half-angle intermediate (including the source OneEighty product).
    let (s, c) = (heading * 0.5).sin_cos();
    let z2 = s + s;
    Vec3::new(
        v.x * (1.0 - s * z2) - v.y * (c * z2),
        v.x * (c * z2) + v.y * (1.0 - s * z2),
        v.z,
    )
}

/// ACE CalculateProjectileVelocity Strike path, with immutable accepted inputs.
pub fn ace_strike_velocity(
    start: Vec3,
    aim: Vec3,
    target_velocity: Vec3,
    speed: f32,
) -> Result<Vec3, E> {
    if !start.is_finite()
        || !aim.is_finite()
        || !target_velocity.is_finite()
        || !speed.is_finite()
        || speed <= 0.0
    {
        return Err(E::InvalidGeometry);
    }
    let offset = aim - start;
    let straight = unit(offset).ok_or(E::InvalidGeometry)? * speed;
    if target_velocity == Vec3::ZERO {
        return Ok(straight);
    }
    // Original Trajectory.solve_ballistic_arc_lateral + SolveQuadric. No
    // alternate configured solver is silently selected for this gap-fill.
    let a = f64::from(
        target_velocity.x * target_velocity.x + target_velocity.y * target_velocity.y
            - speed * speed,
    );
    let b = f64::from(2.0 * (offset.x * target_velocity.x + offset.y * target_velocity.y));
    let c = f64::from(offset.x * offset.x + offset.y * offset.y);
    let p = b / (2.0 * a);
    let q = c / a;
    let disc = p * p - q;
    let roots = if disc.abs() < 1e-9 {
        [-p, f64::NAN]
    } else if disc < 0.0 {
        [f64::NAN; 2]
    } else {
        [disc.sqrt() - p, -disc.sqrt() - p]
    };
    let Some(t) = roots
        .into_iter()
        .filter(|v| *v > 0.0 && v.is_finite())
        .map(|v| v as f32)
        .reduce(f32::min)
    else {
        return Ok(straight);
    };
    let impact = aim + target_velocity * t;
    let dir = impact - start;
    let Some(horizontal) = unit(Vec3::new(dir.x, dir.y, 0.0)) else {
        return Ok(straight);
    };
    let mut velocity = horizontal * speed;
    velocity.z = -(2.0 * start.z - 2.0 * impact.z) / (t * 2.0);
    if !velocity.is_finite() {
        return Err(E::InvalidGeometry);
    }
    Ok(velocity)
}

pub fn ace_strike_projectiles(
    spec: &ProjectileSpec,
    source: ProjectileActorFrame,
    target: ProjectileActorFrame,
    draws: &[Vec3],
) -> Result<Vec<ProjectileLaunch>, E> {
    if spec.shape != ProjectileShape::Strike || spec.gravity != 0.0 {
        return Err(E::InvalidSpec);
    }
    if !valid(source) || !valid(target) {
        return Err(E::InvalidGeometry);
    }
    let offset = target.position - source.position;
    let pre = projectile_pre_offset(
        spec.shape,
        source.height,
        source.radius,
        spec.radius,
        Some((offset, target.height)),
    )?;
    let mut origins = Vec::with_capacity(usize::from(spec.count));
    projectile_origins(
        spec,
        source.radius,
        pre,
        Some(offset.length_squared().sqrt() - source.radius - target.radius),
        spec.perturbation,
        draws,
        &mut origins,
    )?;
    let heading = (-offset.x).atan2(offset.y);
    // rotate * OneEighty has q.z=cos(heading/2), q.w=-sin(heading/2),
    // apart from the original f32 PI's tiny cosine. Multiplying quaternions
    // explicitly preserves that boundary instead of adding angles.
    let transform = |v: Vec3| {
        let (sz, cz) = (heading * 0.5).sin_cos();
        let (sp, cp) = (std::f32::consts::PI * 0.5).sin_cos();
        let z = sz * cp + sp * cz;
        let w = cz * cp - sz * sp;
        Vec3::new(
            v.x * (1.0 - 2.0 * z * z) - v.y * (2.0 * w * z),
            v.x * (2.0 * w * z) + v.y * (1.0 - 2.0 * z * z),
            v.z,
        )
    };
    let first = *origins.first().ok_or(E::InvalidSpec)?;
    let start = target.position + transform(first);
    let aim = target.position + Vec3::new(0.0, 0.0, target.height * (2.0 / 3.0));
    let velocity = ace_strike_velocity(
        start,
        aim,
        if spec.tracking {
            target.velocity
        } else {
            Vec3::ZERO
        },
        spec.speed,
    )?;
    let mut out = Vec::with_capacity(origins.len());
    for origin in origins {
        let position = target.position + transform(origin);
        let velocity = if spec.spread_degrees > 0.0 {
            rotate(velocity, (-origin.x).atan2(origin.y))
        } else {
            velocity
        };
        if !position.is_finite() {
            return Err(E::InvalidGeometry);
        }
        out.push(ProjectileLaunch { position, velocity });
    }
    Ok(out)
}
