//! Cast recovery is actor state, never attempt state. GDLE SpellcastingManager
//! NEXT_SPELLCAST_TIMESTAMP (2 seconds); ACE Player_Magic.GetCastingPreCheckStatus
//! War/Void switching (a fresh 3..5 second draw per attempt). Explicit clocks only.
use crate::{CastError, MagicSchool};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CastRecovery {
    pub revision: u64,
    pub minimum_remaining: f64,
    pub streak_remaining: f64,
    pub last_success_school: Option<MagicSchool>,
    /// Saturates at five seconds: older history cannot affect school recovery.
    pub last_success_age: f64,
}
impl CastRecovery {
    pub fn validate(self) -> Result<(), CastError> {
        if !self.minimum_remaining.is_finite()
            || !(0.0..=4.0).contains(&self.minimum_remaining)
            || !self.streak_remaining.is_finite()
            || !(0.0..=2.0).contains(&self.streak_remaining)
            || !self.last_success_age.is_finite()
            || !(0.0..=5.0).contains(&self.last_success_age)
            || self.last_success_school.is_none() && self.last_success_age != 0.0
        {
            return Err(CastError::InvalidPreparation);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct CastRecoveryClock {
    revision: u64,
    minimum_until: f64,
    streak_until: f64,
    last_success: Option<(MagicSchool, f64)>,
}
impl CastRecoveryClock {
    pub fn restore(
        snapshot: CastRecovery,
        now: f64,
        elapsed_offline: f64,
    ) -> Result<Self, CastError> {
        snapshot.validate()?;
        if !now.is_finite() || now < 0.0 || !elapsed_offline.is_finite() || elapsed_offline < 0.0 {
            return Err(CastError::InvalidTime);
        }
        Ok(Self {
            revision: snapshot.revision,
            minimum_until: now + (snapshot.minimum_remaining - elapsed_offline).max(0.0),
            streak_until: now + (snapshot.streak_remaining - elapsed_offline).max(0.0),
            last_success: snapshot.last_success_school.map(|s| {
                (
                    s,
                    now - (snapshot.last_success_age + elapsed_offline).min(5.0),
                )
            }),
        })
    }
    pub fn snapshot(self, now: f64) -> Result<CastRecovery, CastError> {
        if !now.is_finite() || now < 0.0 || self.last_success.is_some_and(|(_, t)| now < t) {
            return Err(CastError::InvalidTime);
        }
        let result = CastRecovery {
            revision: self.revision,
            minimum_remaining: (self.minimum_until - now).max(0.0),
            streak_remaining: (self.streak_until - now).max(0.0),
            last_success_school: self.last_success.map(|s| s.0),
            last_success_age: self.last_success.map_or(0.0, |(_, t)| (now - t).min(5.0)),
        };
        result.validate()?;
        Ok(result)
    }
    pub fn deadlines(self) -> (f64, f64) {
        (self.minimum_until, self.streak_until)
    }
    pub fn can_mutate(self) -> bool {
        self.revision < u64::MAX
    }
    pub fn record(
        &mut self,
        minimum_until: f64,
        streak_until: f64,
        success: Option<MagicSchool>,
        now: f64,
    ) -> Result<(), CastError> {
        if !now.is_finite()
            || now < 0.0
            || !minimum_until.is_finite()
            || !streak_until.is_finite()
            || minimum_until < 0.0
            || streak_until < 0.0
            || minimum_until > now + 4.0
            || streak_until > now + 2.0
        {
            return Err(CastError::InvalidTime);
        }
        let next = self
            .revision
            .checked_add(1)
            .ok_or(CastError::SequenceExhausted)?;
        self.minimum_until = minimum_until;
        self.streak_until = streak_until;
        if let Some(school) = success {
            self.last_success = Some((school, now));
        }
        self.revision = next;
        Ok(())
    }
    pub fn school_locked(
        self,
        school: MagicSchool,
        now: f64,
        unit_draw: f64,
    ) -> Result<bool, CastError> {
        if !now.is_finite()
            || now < 0.0
            || !unit_draw.is_finite()
            || !(0.0..=1.0).contains(&unit_draw)
        {
            return Err(CastError::InvalidTime);
        }
        let Some((previous, at)) = self.last_success else {
            return Ok(false);
        };
        if now < at {
            return Err(CastError::InvalidTime);
        }
        let opposite = matches!(
            (previous, school),
            (MagicSchool::War, MagicSchool::Void) | (MagicSchool::Void, MagicSchool::War)
        );
        Ok(opposite && now - at < f64::from(3.0_f32 + unit_draw as f32 * 2.0))
    }
}
