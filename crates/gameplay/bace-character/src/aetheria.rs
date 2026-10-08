//! ACE Entity/Aetheria.cs, Entity/ExperienceSystem.cs and WorldObject_Set.cs,
//! pinned 47edade3bd3f6044b676d4eb877c4965c7eda62b, AGPL-3.0-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemExperienceStyle {
    Fixed,
    ScalesWithLevel,
    FixedPlusBase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AetheriaError {
    Invalid,
    Overflow,
    Conflict,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemExperience {
    pub total: u64,
    pub base: u64,
    pub maximum_level: u32,
    pub style: ItemExperienceStyle,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemExperienceChange {
    pub before: ItemExperience,
    pub after: ItemExperience,
    pub before_level: u32,
    pub after_level: u32,
    pub added: u64,
}
impl ItemExperience {
    pub fn xp_for_level(&self, level: u32) -> Result<u64, AetheriaError> {
        if self.base == 0 || self.maximum_level == 0 || self.maximum_level > 64 {
            return Err(AetheriaError::Invalid);
        }
        let level = level.min(self.maximum_level);
        if level == 0 {
            return Ok(0);
        }
        if level == 1 {
            return Ok(self.base);
        }
        let xp = match self.style {
            ItemExperienceStyle::Fixed => u64::from(level).checked_mul(self.base),
            ItemExperienceStyle::FixedPlusBase => u64::from(level)
                .checked_mul(self.base)
                .and_then(|v| v.checked_add(self.base)),
            ItemExperienceStyle::ScalesWithLevel => {
                let mut sum = self.base;
                let mut next = self.base;
                for _ in 1..level {
                    next = next.checked_mul(2).ok_or(AetheriaError::Overflow)?;
                    sum = sum.checked_add(next).ok_or(AetheriaError::Overflow)?;
                }
                Some(sum)
            }
        }
        .ok_or(AetheriaError::Overflow)?;
        if xp > i64::MAX as u64 {
            return Err(AetheriaError::Overflow);
        }
        Ok(xp)
    }
    pub fn level(&self) -> Result<u32, AetheriaError> {
        let maximum = self.xp_for_level(self.maximum_level)?;
        if self.total > maximum {
            return Err(AetheriaError::Invalid);
        }
        let level = match self.style {
            ItemExperienceStyle::Fixed => (self.total as f64 / self.base as f64).floor() as u32,
            ItemExperienceStyle::FixedPlusBase => {
                if self.total < self.base {
                    0
                } else if self.total < self.base.checked_mul(3).ok_or(AetheriaError::Overflow)? {
                    1
                } else {
                    ((self.total - self.base) as f64 / self.base as f64).floor() as u32
                }
            }
            ItemExperienceStyle::ScalesWithLevel => {
                let mut level = 0;
                let mut remaining = self.total;
                let mut xp = self.base;
                while remaining >= xp {
                    remaining -= xp;
                    level += 1;
                    if level >= self.maximum_level {
                        break;
                    }
                    xp = xp.checked_mul(2).ok_or(AetheriaError::Overflow)?;
                }
                level
            }
        };
        Ok(level.min(self.maximum_level))
    }
    pub fn propose_xp(self, amount: u64) -> Result<ItemExperienceChange, AetheriaError> {
        let before_level = self.level()?;
        let maximum = self.xp_for_level(self.maximum_level)?;
        let total = self
            .total
            .checked_add(amount)
            .ok_or(AetheriaError::Overflow)?
            .min(maximum);
        let added = total - self.total;
        let revision = if added == 0 {
            self.revision
        } else {
            self.revision
                .checked_add(1)
                .ok_or(AetheriaError::Overflow)?
        };
        let after = Self {
            total,
            revision,
            ..self
        };
        Ok(ItemExperienceChange {
            before: self,
            after,
            before_level,
            after_level: after.level()?,
            added,
        })
    }
    pub fn adopt(&mut self, change: ItemExperienceChange) -> Result<(), AetheriaError> {
        if *self != change.before || self.propose_xp(change.added)? != change {
            return Err(AetheriaError::Conflict);
        }
        *self = change.after;
        Ok(())
    }
}
pub fn aetheria_proc_rate(
    level: u32,
    luminance_surge_aug: u32,
    combat_mode: u32,
) -> Result<f32, AetheriaError> {
    if level > 64 || luminance_surge_aug > 10000 {
        return Err(AetheriaError::Invalid);
    }
    let mut chance = level as f32 * 0.01f32;
    chance += luminance_surge_aug as f32 * 0.001f32;
    match combat_mode {
        4 => chance *= 1.5,
        8 => chance *= 2.0,
        1 | 2 => {}
        _ => return Err(AetheriaError::Invalid),
    }
    Ok(chance)
}
pub fn aetheria_proc(
    level: u32,
    aug: u32,
    combat_mode: u32,
    draw: f64,
) -> Result<bool, AetheriaError> {
    if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
        return Err(AetheriaError::Invalid);
    }
    Ok(draw < f64::from(aetheria_proc_rate(level, aug, combat_mode)?))
}
pub fn aetheria_surge_self_target(spell: u32) -> Result<bool, AetheriaError> {
    match spell {
        5204 | 5206 | 5208 => Ok(true),
        5205 | 5207 => Ok(false),
        _ => Err(AetheriaError::Invalid),
    }
}
