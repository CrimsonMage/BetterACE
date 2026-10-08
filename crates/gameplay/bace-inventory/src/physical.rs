//! ACE Player_Inventory.GetPickupMotion and Physics.Common.Position.CylinderDistance.
//! Explicit accepted geometry only; client-reported poses cannot authorize use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InventoryCylinder {
    pub position: [f32; 3],
    pub radius: f32,
    pub height: f32,
}
pub fn inventory_use_distance(a: InventoryCylinder, b: InventoryCylinder) -> Option<f64> {
    if a.position
        .into_iter()
        .chain(b.position)
        .chain([a.radius, a.height, b.radius, b.height])
        .any(|v| !v.is_finite())
        || [a.radius, a.height, b.radius, b.height]
            .into_iter()
            .any(|v| v < 0.)
    {
        return None;
    }
    let d = [
        b.position[0] - a.position[0],
        b.position[1] - a.position[1],
        b.position[2] - a.position[2],
    ];
    let reach = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - (a.radius + b.radius);
    let z = if a.position[2] <= b.position[2] {
        b.position[2] - (a.position[2] + a.height)
    } else {
        a.position[2] - (b.position[2] + b.height)
    };
    Some(if z > 0. && reach > 0. {
        (f64::from(z * z + reach * reach)).sqrt()
    } else if z < 0. && reach < 0. {
        -(f64::from(z * z + reach * reach)).sqrt()
    } else {
        f64::from(reach)
    })
}
pub fn inventory_pickup_motion(
    player_z: f32,
    player_height: f32,
    target_z: f32,
    target_height: f32,
    corpse: bool,
) -> Option<u32> {
    if [player_z, player_height, target_z, target_height]
        .into_iter()
        .any(|v| !v.is_finite())
        || player_height < 0.
        || target_height < 0.
    {
        return None;
    }
    let z = target_z + if corpse { 0. } else { target_height * 0.5 };
    for (ratio, motion) in [
        (0.9, 0x40000139),
        (0.7, 0x40000138),
        (0.5, 0x40000137),
        (0.2, 0x40000136),
    ] {
        if f64::from(z) >= f64::from(player_z) + f64::from(player_height) * ratio {
            return Some(motion);
        }
    }
    Some(0x40000018)
}
