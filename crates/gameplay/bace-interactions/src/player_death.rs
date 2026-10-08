//! ACE Player_Death and EnchantmentManager death rules, AGPL-3.0-only.
//! A prepared result is not a durable receipt; owners must commit inventory,
//! registry and player changes together before progressing the death sequence.
use crate::{PortalPosition, RecallError};
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerDeathPolicy {
    pub vitae_penalty: f64,
    pub vitae_penalty_max: f64,
    pub destroy_pyreals: bool,
    pub lifestone_broadcast: bool,
    pub pk_respite_seconds: u32,
    pub pk_server: bool,
    pub pkl_server: bool,
    pub safe_training_academy: bool,
}
impl Default for PlayerDeathPolicy {
    fn default() -> Self {
        Self {
            vitae_penalty: 0.05,
            vitae_penalty_max: 0.4,
            destroy_pyreals: true,
            lifestone_broadcast: true,
            pk_respite_seconds: 300,
            pk_server: false,
            pkl_server: false,
            safe_training_academy: false,
        }
    }
}
impl PlayerDeathPolicy {
    pub fn validate(&self) -> Result<(), RecallError> {
        if !self.vitae_penalty.is_finite()
            || !self.vitae_penalty_max.is_finite()
            || !(0.0..=1.0).contains(&self.vitae_penalty)
            || !(0.0..1.0).contains(&self.vitae_penalty_max)
            || self.vitae_penalty > self.vitae_penalty_max
            || self.pk_respite_seconds > 86400
            || self.pk_server && self.pkl_server
        {
            return Err(RecallError::Invalid);
        }
        Ok(())
    }
    pub fn next_vitae(&self, level: u32, current: Option<f32>) -> Result<f32, RecallError> {
        self.validate()?;
        if level == 0
            || level > 275
            || current.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err(RecallError::Invalid);
        }
        let base = 1.0 - self.vitae_penalty_max;
        let max_penalty = ((level - 1) * 3)
            .max(1)
            .min(100 - (base * 100.).round_ties_even() as u32);
        let min_vitae = ((100 - max_penalty) as f32 / 100.).max(base as f32);
        Ok((current.unwrap_or(1.) - self.vitae_penalty as f32).clamp(min_vitae, 1.))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerDeathKind {
    Ordinary,
    Pk,
    Pkl,
}
pub fn classify_player_death(victim: u32, pk_status: u32, killer: Option<u32>) -> PlayerDeathKind {
    // ACE ObjectGuid.IsPlayer uses the 0x50000000 identity range.
    let player_killer =
        killer.is_some_and(|id| id != victim && (0x50000001..0x60000000).contains(&id));
    if player_killer && pk_status & 4 != 0 {
        PlayerDeathKind::Pk
    } else if player_killer && pk_status & 64 != 0 {
        PlayerDeathKind::Pkl
    } else {
        PlayerDeathKind::Ordinary
    }
}
pub fn keep_death_enchantment(
    duration: f64,
    spell: u32,
    beneficial: bool,
    kind: PlayerDeathKind,
    retain_augmentation: bool,
) -> bool {
    duration == -1.
        || spell > i16::MAX as u32
        || (kind != PlayerDeathKind::Pk && retain_augmentation && beneficial)
}
pub fn death_destination(
    sanctuary: Option<PortalPosition>,
    instantiation: Option<PortalPosition>,
    location: PortalPosition,
) -> Result<PortalPosition, RecallError> {
    let destination = sanctuary.or(instantiation).unwrap_or(location);
    destination.validate().map_err(|_| RecallError::Invalid)?;
    Ok(destination)
}
pub fn death_item_count(
    level: u32,
    draw: u32,
    less_loss_augmentation: u32,
    pk_death: bool,
) -> Result<u32, RecallError> {
    if level == 0
        || level > 275
        || less_loss_augmentation > 3
        || draw > if level <= 20 { 1 } else { 2 }
    {
        return Err(RecallError::Invalid);
    }
    if level <= 10 {
        return Ok(0);
    }
    if level <= 20 {
        return Ok(draw);
    }
    let count = (level / 20 + draw).min(14);
    Ok(if pk_death {
        count
    } else {
        count.saturating_sub(less_loss_augmentation * 5)
    })
}
pub fn death_coin_count(level: u32, coins: u32) -> u32 {
    if level > 5 { coins / 2 } else { 0 }
}
pub fn restored_death_vital(modified_max: u32) -> u32 {
    (modified_max as f32 * 0.75).round_ties_even() as u32
}
pub fn player_corpse_decay_seconds(level: u32, empty: bool) -> u64 {
    if empty { 15 } else { u64::from(level) * 300 }.max(if empty { 15 } else { 3600 })
}
