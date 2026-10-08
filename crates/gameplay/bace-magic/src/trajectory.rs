//! GDLE GetSpellProjectileSpawnVelocity's launch-time leading, with explicit
//! accepted target velocity. Degenerate/nonfinite roots use its direct fallback.
use crate::ProjectileLayoutError;
use bace_geometry::Vec3;
pub fn projectile_velocity(
    offset: Vec3,
    target_velocity: Vec3,
    speed: f32,
    tracking: bool,
    gravity: f32,
) -> Result<Vec3, ProjectileLayoutError> {
    if !offset.is_finite()
        || !target_velocity.is_finite()
        || !speed.is_finite()
        || speed <= 0.0
        || !gravity.is_finite()
        || gravity < 0.0
    {
        return Err(ProjectileLayoutError::InvalidGeometry);
    }
    let distance = offset.length_squared().sqrt();
    if !distance.is_finite() || distance <= 1e-6 {
        return Err(ProjectileLayoutError::InvalidGeometry);
    }
    let gravity64 = if gravity == 9.8_f32 {
        9.8_f64
    } else {
        f64::from(gravity)
    };
    let direct = || {
        let t = f64::from(distance) / f64::from(speed);
        Vec3::new(
            offset.x / t as f32,
            offset.y / t as f32,
            (f64::from(offset.z / t as f32) + gravity64 * t / 2.0) as f32,
        )
    };
    let result = if !tracking {
        direct()
    } else {
        let target_speed = target_velocity.length_squared().sqrt();
        let unit = if target_speed < 0.0002 {
            Vec3::ZERO
        } else {
            target_velocity * (1.0 / target_speed)
        };
        let a = f64::from(unit.x * unit.x + unit.y * unit.y - speed * speed);
        let b = f64::from(2.0 * (offset.x * unit.x + offset.y * unit.y));
        let c = f64::from(offset.x * offset.x + offset.y * offset.y);
        let discriminant = b * b - 4.0 * a * c;
        if a == 0.0 || !discriminant.is_finite() || discriminant < 0.0 {
            direct()
        } else {
            let mut t1 = (-b + discriminant.sqrt()) / (2.0 * a);
            let mut t2 = (-b - discriminant.sqrt()) / (2.0 * a);
            if t1 < 0.0 {
                t1 = f64::from(f32::MAX);
            }
            if t2 < 0.0 {
                t2 = f64::from(f32::MAX);
            }
            let t = t1.min(t2);
            if !t.is_finite() || t <= 1e-6 || t >= 100.0 {
                direct()
            } else {
                Vec3::new(
                    ((f64::from(offset.x) + t * f64::from(target_speed) * f64::from(unit.x)) / t)
                        as f32,
                    ((f64::from(offset.y) + t * f64::from(target_speed) * f64::from(unit.y)) / t)
                        as f32,
                    ((f64::from(offset.z) + t * f64::from(target_speed) * f64::from(unit.z)) / t
                        + gravity64 * t / 2.0) as f32,
                )
            }
        }
    };
    if !result.is_finite() {
        return Err(ProjectileLayoutError::InvalidGeometry);
    }
    Ok(result)
}
