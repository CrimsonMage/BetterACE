use super::PhysicalError;
/// CMissileAttackEvent::CalculateMissileVelocity, scalar operation order. Invalid
/// roots/zero-time targets fail closed instead of producing nonfinite physics.
pub fn missile_velocity(
    p: [f32; 3],
    target_velocity: [f32; 3],
    tracking: bool,
    gravity: bool,
    speed: f32,
) -> Result<[f32; 3], PhysicalError> {
    if !p.into_iter().chain(target_velocity).all(f32::is_finite)
        || !speed.is_finite()
        || speed <= 0.0
    {
        return Err(PhysicalError::InvalidInput);
    }
    let distance = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    if distance <= 0.0 {
        return Err(PhysicalError::InvalidInput);
    }
    let direct = || {
        let t = distance / speed;
        let mut v = [p[0] / t, p[1] / t, p[2] / t];
        if gravity {
            v[2] += (9.8 * t) / 2.0;
        }
        v
    };
    let mut v = direct();
    if tracking {
        let s = (target_velocity[0] * target_velocity[0]
            + target_velocity[1] * target_velocity[1]
            + target_velocity[2] * target_velocity[2])
            .sqrt();
        let n = if s < 0.0002 {
            [0.0; 3]
        } else {
            [
                target_velocity[0] / s,
                target_velocity[1] / s,
                target_velocity[2] / s,
            ]
        };
        let a = n[0] * n[0] + n[1] * n[1] - speed * speed;
        let b = 2.0 * (p[0] * n[0] + p[1] * n[1]);
        let c = p[0] * p[0] + p[1] * p[1];
        let det = b * b - 4.0 * a * c;
        if a != 0.0 && det >= 0.0 {
            let t1 = (-b + det.sqrt()) / (2.0 * a);
            let t2 = (-b - det.sqrt()) / (2.0 * a);
            let t =
                (if t1 < 0.0 { f32::MAX } else { t1 }).min(if t2 < 0.0 { f32::MAX } else { t2 });
            if t > 0.0 && t < 100.0 {
                let ts = t * s;
                v = [
                    (ts * n[0] + p[0]) / t,
                    (ts * n[1] + p[1]) / t,
                    (ts * n[2] + p[2]) / t,
                ];
                if gravity {
                    v[2] += (9.8 * t) / 2.0;
                }
            } else {
                v = direct();
                v[2] = p[2] / (distance / speed) + (9.8 * (distance / speed)) / 2.0;
            }
        } else {
            return Err(PhysicalError::InvalidInput);
        }
    }
    if v.into_iter().all(f32::is_finite) {
        Ok(v)
    } else {
        Err(PhysicalError::InvalidInput)
    }
}
