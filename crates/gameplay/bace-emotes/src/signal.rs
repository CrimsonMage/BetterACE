//! Pinned ACE Physics.Common.Position.CylinderDistance (47edade3...) used by
//! Landblock.EmitSignal. Preserve the source's float intermediates, including
//! its full 3D offset length before the signed vertical correction.
use bace_geometry::Vec3;
#[derive(Clone, Copy, Debug)]
pub struct SignalCylinder {
    pub position: Vec3,
    pub radius: f32,
    pub height: f32,
}
pub fn cylinder_distance(a: SignalCylinder, b: SignalCylinder) -> Result<f64, crate::NativeError> {
    if [a, b].iter().any(|c| {
        !c.position.is_finite()
            || !c.radius.is_finite()
            || !c.height.is_finite()
            || c.radius < 0.0
            || c.height < 0.0
    }) {
        return Err(crate::NativeError::InvalidContent);
    }
    let offset = b.position - a.position;
    let reach = offset.length_squared().sqrt() - (a.radius + b.radius);
    let z = if a.position.z <= b.position.z {
        b.position.z - (a.position.z + a.height)
    } else {
        a.position.z - (b.position.z + b.height)
    };
    if !reach.is_finite() || !z.is_finite() {
        return Err(crate::NativeError::InvalidContent);
    }
    let result = if z > 0.0 && reach > 0.0 {
        f64::from(z * z + reach * reach).sqrt()
    } else if z < 0.0 && reach < 0.0 {
        -f64::from(z * z + reach * reach).sqrt()
    } else {
        f64::from(reach)
    };
    if result.is_finite() {
        Ok(result)
    } else {
        Err(crate::NativeError::InvalidContent)
    }
}
