//! Explicit-time physical combat policy. GDLE AttackManager/AttackEventData and
//! Ammunition at 353cbab52ef7da2b7063bc3e3f008461d8531693. Dual-wield charge
//! follows ClientCombatSystem::GetPowerBarLevel (0056ADE0), per owner policy.
use super::PhysicalError;

pub fn charge_seconds(power: f32, dual_wield: bool) -> Result<f64, PhysicalError> {
    if !power.is_finite() || !(0.0..=1.0).contains(&power) {
        return Err(PhysicalError::InvalidInput);
    }
    Ok(f64::from(power) * if dual_wield { 0.8 } else { 1.0 })
}

/// CharacterOptions2 is sampled when the missile is launched. No homing occurs.
pub fn missile_settings(
    base_speed: f32,
    player: bool,
    options2: u32,
) -> Result<(f32, bool), PhysicalError> {
    if !base_speed.is_finite() || base_speed <= 0.0 {
        return Err(PhysicalError::InvalidInput);
    }
    let speed = base_speed
        * if player && options2 & 0x10000 != 0 {
            1.25
        } else {
            0.9
        };
    if !speed.is_finite() {
        return Err(PhysicalError::InvalidInput);
    }
    Ok((speed, player && options2 & 0x8000 != 0))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MissileLifetime {
    rot_at: f64,
    destroy_emitted: bool,
    stopped: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissileLifetimeEvent {
    Waiting,
    DestroyEffect,
    Remove,
}
impl MissileLifetime {
    pub fn launch(now: f64) -> Result<Self, PhysicalError> {
        if !now.is_finite() || now < 0.0 || !(now + 7.0).is_finite() {
            return Err(PhysicalError::InvalidInput);
        }
        Ok(Self {
            rot_at: now + 5.0,
            destroy_emitted: false,
            stopped: false,
        })
    }
    pub fn environment(&mut self, now: f64) -> Result<(), PhysicalError> {
        if !now.is_finite() || now < 0.0 || !(now + 3.0).is_finite() {
            return Err(PhysicalError::InvalidInput);
        }
        self.rot_at = now + 1.0;
        self.destroy_emitted = false;
        self.stopped = true;
        Ok(())
    }
    pub fn stopped(self) -> bool {
        self.stopped
    }
    /// Caller preflights bounded output before advancing this state.
    pub fn poll(&mut self, now: f64) -> Result<MissileLifetimeEvent, PhysicalError> {
        if !now.is_finite() || now < 0.0 {
            return Err(PhysicalError::InvalidInput);
        }
        if now < self.rot_at {
            return Ok(MissileLifetimeEvent::Waiting);
        }
        if !self.destroy_emitted {
            self.destroy_emitted = true;
            return Ok(MissileLifetimeEvent::DestroyEffect);
        }
        Ok(if now >= self.rot_at + 2.0 {
            MissileLifetimeEvent::Remove
        } else {
            MissileLifetimeEvent::Waiting
        })
    }
}
