//! Official ACE 47edade3 Creature_Missile.GetMaxMissileRange and
//! Creature_Combat.GetAnimSpeed, AGPL-3.0-only. Cold NPC preparation only.
use super::PhysicalError;
pub fn npc_missile_range(maximum_velocity: f64) -> Result<f32, PhysicalError> {
    if !maximum_velocity.is_finite() || !(0.0..=1000.0).contains(&maximum_velocity) {
        return Err(PhysicalError::InvalidInput);
    }
    Ok(((maximum_velocity.powi(2) as f32) * 0.102_040_82).min(85.0 / 1.094))
}
pub fn npc_reload_speed(quickness: u32, weapon_speed: u32) -> f32 {
    let divisor = 1.0 - f64::from(quickness) / 300.0 + f64::from(weapon_speed) / 150.0;
    if divisor <= 0.0 {
        2.0
    } else {
        (1.0 / divisor).clamp(0.5, 2.0) as f32
    }
}
