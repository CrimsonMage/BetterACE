//! Projectile dimensions and timing come from the verified DAT preparation.
//! Pinned Creature_Missile.GetProjectileSpeed/GetProjectileVelocity and
//! Monster_Missile.LaunchMissile supply fallback speed, gravity and target lead.
use bace_gameplay_api::weapon_combat::PhysicalMissileSpec;
use bace_loot::PreparedCreatureEquipment;
use std::{collections::BTreeMap, sync::Arc};
pub fn prepare_creature_missile(
    prepared: &bace_simulation::GeneratedNpcTemplate,
    items: &[(bace_types::EntityId, PreparedCreatureEquipment)],
    shapes: &BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
) -> Result<Option<PhysicalMissileSpec>, String> {
    let Some((_, launcher)) = items
        .iter()
        .find(|(_, item)| item.wielded_location == 0x400000)
    else {
        return Ok(None);
    };
    let int = |w: &bace_content::WeenieV1, id, default| {
        w.properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(default, |p| p.value)
    };
    let float = |w: &bace_content::WeenieV1, id, default| {
        w.properties
            .floats
            .iter()
            .find(|p| p.id == id)
            .map_or(default, |p| p.value)
    };
    let style = match int(&launcher.source, 46, 0) {
        0x10 => 0x8000003f,
        0x20 => 0x80000041,
        0x40 => 0x80000043,
        0x80 | 0x800 => 0x80000047,
        0x400 => 0x8000013b,
        _ => return Err("unsupported source missile stance".into()),
    };
    let ammo = if matches!(int(&launcher.source, 46, 0), 0x80 | 0x800) {
        launcher
    } else {
        &items
            .iter()
            .find(|(_, item)| item.wielded_location == 0x800000)
            .ok_or("source missile weapon has no accepted ammunition")?
            .1
    };
    let shape = shapes
        .get(&ammo.source.weenie_id)
        .ok_or("missing verified projectile setup")?;
    let radius = shape
        .spheres()
        .first()
        .ok_or("projectile setup has no source sphere")?
        .radius;
    let mut speed = float(&launcher.source, 26, 20.0) as f32;
    if speed == 0.0 {
        speed = 20.0;
    }
    let timing = prepared
        .missile_timings
        .iter()
        .find(|m| m.style == style)
        .ok_or_else(|| {
            prepared
                .missile_timing_errors
                .iter()
                .find(|(s, _)| *s == style)
                .map_or_else(
                    || "missing prepared missile aim/reload motion".to_string(),
                    |(_, e)| format!("unsupported missile timing: {e}"),
                )
        })?;
    let base = prepared
        .physical
        .as_ref()
        .ok_or("missing prepared creature physical profile")?;
    // Source GetWeaponSpeed(wielder as Player) uses default 40 for monsters;
    // authoritative aura speed changes are supplied by later live projections.
    let animation_speed = bace_combat::physical::npc_reload_speed(base.quickness, 40);
    let result = PhysicalMissileSpec {
        ammunition_count: u32::try_from(int(&ammo.source, 12, 1))
            .map_err(|_| "negative ammunition stack")?,
        speed,
        radius,
        gravity: true,
        tracking: true,
        attack_motion: 0x4000001e,
        launch_seconds: timing.launch_seconds,
        duration_seconds: timing.launch_seconds
            + timing.reload_seconds / f64::from(animation_speed)
            + timing.return_seconds,
        damage_modifier: float(&launcher.source, 63, 1.0),
    };
    if !result.speed.is_finite()
        || result.speed <= 0.0
        || result.speed > 1000.0
        || !radius.is_finite()
        || radius <= 0.0
        || result.ammunition_count == 0
        || !result.duration_seconds.is_finite()
        || result.duration_seconds > 120.0
        || !result.damage_modifier.is_finite()
        || result.damage_modifier < 0.0
    {
        return Err("invalid prepared projectile specification".into());
    }
    Ok(Some(result))
}
