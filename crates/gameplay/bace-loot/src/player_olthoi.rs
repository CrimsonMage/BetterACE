//! Pinned ACE LootGenerationFactory_OlthoiPlay player overloads, AGPL-3.0-only.
//! Proposed timestamp updates are committed with the death operation.
use crate::treasure_random::{inclusive, unit};
use crate::{TreasureError, TreasureRandom};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerSlag {
    pub stack: u32,
    pub timestamp: i32,
}
/// The source repeats a roll with probability .05. A bounded 1024-iteration
/// ceiling rejects an adversarial/random-source failure without advancing RNG.
pub fn roll_player_slag<R: TreasureRandom>(
    level: u32,
    had_vitae: bool,
    previous_timestamp: i32,
    now: i32,
    random: &mut R,
) -> Result<Option<PlayerSlag>, TreasureError> {
    if level > 275 || now < 0 || previous_timestamp < 0 {
        return Err(TreasureError::Bounds);
    }
    if had_vitae || level < 100 {
        return Ok(None);
    }
    let maximum = match level {
        100..=134 => 5,
        135..=184 => 10,
        _ => 15,
    };
    let mut cursor = random.clone();
    let mut total = 0i32;
    let mut completed = false;
    for _ in 0..1024 {
        total = total
            .checked_add(inclusive(&mut cursor, 1, maximum)?)
            .ok_or(TreasureError::Bounds)?;
        if unit(&mut cursor)? >= f64::from(0.05f32) {
            completed = true;
            break;
        }
    }
    if !completed {
        return Err(TreasureError::Capacity);
    }
    let elapsed = (now - previous_timestamp).max(0);
    if elapsed < 3600 {
        let scale = (f64::from(elapsed) / 3600.) as f32;
        total = (total as f32 * scale).round() as i32;
    }
    *random = cursor;
    Ok((total > 0).then_some(PlayerSlag {
        stack: total as u32,
        timestamp: now,
    }))
}
/// Source invokes this after its normal Olthoi death treasure rolls.
pub fn roll_player_gland<R: TreasureRandom>(
    had_vitae: bool,
    random: &mut R,
) -> Result<bool, TreasureError> {
    if had_vitae {
        return Ok(false);
    }
    let mut cursor = random.clone();
    let result = inclusive(&mut cursor, 1, 100)? > 50;
    *random = cursor;
    Ok(result)
}
