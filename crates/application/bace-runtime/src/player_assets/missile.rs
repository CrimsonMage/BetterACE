//! Player launch assets from accepted equipment and the verified motion table.
use super::*;
use bace_combat::preparation::{
    PhysicalEquipment, PhysicalQualityFamily, PhysicalQualityProjection,
};
use bace_gameplay_api::weapon_combat::PhysicalMissileSpec;
pub(super) fn prepare_player_missile(
    dat: &PreparedAvatarDat,
    equipment: &[PhysicalEquipment<'_>],
    qualities: &[PhysicalQualityProjection],
    stats: &PlayerStatProjection,
    shapes: &BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
) -> Result<Option<PhysicalMissileSpec>, String> {
    let Some(launcher) = equipment.iter().find(|e| e.location == 0x400000) else {
        return Ok(None);
    };
    let integer = |w: &bace_content::WeenieV1, id, default| {
        w.properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(default, |p| p.value)
    };
    let floating = |w: &bace_content::WeenieV1, id, default| {
        w.properties
            .floats
            .iter()
            .find(|p| p.id == id)
            .map_or(default, |p| p.value)
    };
    let style = match integer(launcher.weenie, 46, 0) {
        0x10 => 0x8000003f,
        0x20 => 0x80000041,
        0x40 => 0x80000043,
        0x80 | 0x800 => 0x80000047,
        0x400 => 0x8000013b,
        _ => return Err("unsupported player missile stance".into()),
    };
    let ammo = if matches!(integer(launcher.weenie, 46, 0), 0x80 | 0x800) {
        launcher
    } else {
        equipment
            .iter()
            .find(|e| e.location == 0x800000)
            .ok_or("equipped launcher missing ammunition")?
    };
    let shape = shapes
        .get(&ammo.weenie.weenie_id)
        .ok_or("verified ammunition collision shape missing")?;
    let radius = shape
        .spheres()
        .first()
        .ok_or("projectile setup sphere missing")?
        .radius;
    let ready = *dat
        .motions
        .style_defaults
        .get(&style)
        .ok_or("missile ready motion missing")?;
    let timing = |from, to| {
        crate::world_admission::transition_data(&dat.motions, style, from, to)
            .and_then(|data| crate::world_admission::prepare_motion(data, &dat.animations))
            .map(|m| m.duration_seconds())
    };
    let launch_seconds = timing(ready, 0x4000001e)?;
    let reload =
        crate::world_admission::transition_data(&dat.motions, style, 0x4000001e, 0x40000016)
            .ok()
            .map(|d| crate::world_admission::prepare_motion(d, &dat.animations))
            .transpose()?
            .map_or(0., |m| m.duration_seconds());
    let back = timing(if reload > 0. { 0x40000016 } else { 0x4000001e }, ready)?;
    let speed = floating(launcher.weenie, 26, 20.);
    let speed = if speed == 0. { 20. } else { speed };
    let weapon_time = qualities
        .iter()
        .find(|p| {
            p.entity == launcher.entity && p.family == PhysicalQualityFamily::Int && p.stat == 49
        })
        .map_or(f64::from(integer(launcher.weenie, 49, 40)), |p| p.value)
        .max(0.);
    let animation_speed =
        bace_combat::physical::npc_reload_speed(stats.current_attributes[2], weapon_time as u32);
    Ok(Some(PhysicalMissileSpec {
        ammunition_count: u32::try_from(integer(ammo.weenie, 12, 1))
            .map_err(|_| "negative ammunition count")?,
        speed: speed as f32,
        radius,
        gravity: true,
        tracking: true,
        attack_motion: 0x4000001e,
        launch_seconds,
        duration_seconds: launch_seconds + reload / f64::from(animation_speed) + back,
        damage_modifier: floating(launcher.weenie, 63, 1.),
    }))
}
