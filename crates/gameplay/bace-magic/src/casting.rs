//! GDLE SpellcastingManager.cpp at 353cbab52ef7da2b7063bc3e3f008461d8531693.
//! AGPL-3.0-only, GDLE contributors. One bounded cast, explicit clock and accepted
//! physics observations. Client poses/completion messages are never inputs.
use bace_geometry::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CastGesture {
    pub motion: u32,
    pub minimum_seconds: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CastPreparation {
    pub id: u64,
    pub spell: u32,
    pub target: Option<u32>,
    pub gestures: Vec<CastGesture>,
    pub uses_mana: bool,
    pub player: bool,
    pub fast_resistable_pk_spell: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CastObservation {
    pub cell: u32,
    pub position: Vec3,
    pub alive: bool,
    pub in_portal: bool,
    /// Accepted interpreter style is Motion_NonCombat; never inferred from a
    /// requested combat-mode integer in the authentic world adapter.
    pub peace_mode: bool,
    /// Smallest absolute angle from accepted heading to accepted target, degrees.
    /// None for self/untargeted/owned-item casts that require no turn.
    pub heading_to_target: Option<f32>,
    pub target_valid: bool,
    pub target_in_range: bool,
    pub geometry_clear: bool,
    pub turning_to_target: bool,
    pub manual_turning: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastError {
    Busy,
    InvalidPreparation,
    InvalidTime,
    InvalidObservation,
    Dead,
    PortalSpace,
    WrongMode,
    InvalidTarget,
    OutOfRange,
    Obstructed,
    MovementDisrupted,
    MotionFailed,
    TimedOut,
    StaleCompletion,
    NotReleasing,
    SequenceExhausted,
    StreakCooldown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastStage {
    Idle,
    Turning,
    Gesture,
    Release,
    Finished,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CastSignal {
    Waiting,
    Turn {
        cast: u64,
        target: u32,
        sequence: u64,
        maximum_angle_degrees: f32,
    },
    Gesture {
        cast: u64,
        motion: u32,
        index: usize,
        sequence: u64,
        speed: f32,
        stop_movement: bool,
    },
    /// Must be evaluated/committed by the magic/world owners. Emitting this
    /// request does not acknowledge a spell effect or spend any resource.
    Release {
        cast: u64,
        spell: u32,
        target: Option<u32>,
    },
    /// GDLE player Update's peace-style branch: visual fizzle, clamped -5 mana,
    /// LaunchSpellEffect(TRUE) (no normal effect/resources), then successful EndCast.
    PeaceFizzle {
        cast: u64,
        mana_cost: u32,
        intensity: f32,
    },
    MovementFizzle {
        cast: u64,
        mana_cost: u32,
        intensity: f32,
    },
    Finished {
        cast: u64,
        error: Option<CastError>,
    },
}
pub struct CastDriver {
    prepared: Option<CastPreparation>,
    stage: CastStage,
    index: usize,
    origin: Vec3,
    cell: u32,
    timeout: f64,
    next_turn: f64,
    next_cast: f64,
    streak_until: f64,
    release_recorded: bool,
    last_now: f64,
    sequence: u64,
    outstanding: u64,
    turned: bool,
}
impl Default for CastDriver {
    fn default() -> Self {
        Self {
            prepared: None,
            stage: CastStage::Idle,
            index: 0,
            origin: Vec3::ZERO,
            cell: 0,
            timeout: 0.0,
            next_turn: 0.0,
            next_cast: 0.0,
            streak_until: 0.0,
            release_recorded: false,
            last_now: 0.0,
            sequence: 0,
            outstanding: 0,
            turned: false,
        }
    }
}
impl CastDriver {
    pub fn recovery_deadlines(&self) -> (f64, f64) {
        (self.next_cast, self.streak_until)
    }
    pub fn restore_recovery_deadlines(
        &mut self,
        minimum: f64,
        streak: f64,
    ) -> Result<(), CastError> {
        if self.active()
            || !minimum.is_finite()
            || !streak.is_finite()
            || minimum < 0.0
            || streak < 0.0
        {
            return Err(CastError::InvalidPreparation);
        }
        self.next_cast = minimum;
        self.streak_until = streak;
        Ok(())
    }
    pub fn stage(&self) -> CastStage {
        self.stage
    }
    pub fn active(&self) -> bool {
        matches!(
            self.stage,
            CastStage::Turning | CastStage::Gesture | CastStage::Release
        )
    }
    pub fn begin(
        &mut self,
        prepared: CastPreparation,
        now: f64,
        observation: CastObservation,
    ) -> Result<CastSignal, CastError> {
        if self.active() || now < self.next_cast {
            return Err(CastError::Busy);
        }
        self.validate_time(now)?;
        Self::validate_observation(observation)?;
        if prepared.id == 0
            || prepared.spell == 0
            || prepared.gestures.len() > 32
            || prepared.gestures.iter().any(|g| {
                g.motion == 0
                    || !g.minimum_seconds.is_finite()
                    || g.minimum_seconds < 0.0
                    || g.minimum_seconds > 8.0
            })
        {
            return Err(CastError::InvalidPreparation);
        }
        Self::eligibility(observation, instant(&prepared))?;
        self.origin = observation.position;
        self.cell = observation.cell;
        self.index = 0;
        self.timeout = now + 10.0;
        self.next_turn = now;
        self.turned = false;
        self.release_recorded = false;
        self.prepared = Some(prepared);
        self.stage = CastStage::Idle;
        self.last_now = now;
        self.advance(now, observation)
    }
    fn validate_time(&self, now: f64) -> Result<(), CastError> {
        if !now.is_finite() || now < 0.0 || now < self.last_now || !((now + 9999.0).is_finite()) {
            Err(CastError::InvalidTime)
        } else {
            Ok(())
        }
    }
    fn validate_observation(o: CastObservation) -> Result<(), CastError> {
        if o.cell == 0
            || !o.position.is_finite()
            || o.heading_to_target
                .is_some_and(|a| !a.is_finite() || !(0.0..=180.0).contains(&a))
        {
            Err(CastError::InvalidObservation)
        } else {
            Ok(())
        }
    }
    fn eligibility(o: CastObservation, instant: bool) -> Result<(), CastError> {
        if !instant && !o.alive {
            Err(CastError::Dead)
        } else if !instant && o.in_portal {
            Err(CastError::PortalSpace)
        } else if !o.target_valid {
            Err(CastError::InvalidTarget)
        } else if !o.target_in_range {
            Err(CastError::OutOfRange)
        } else if !o.geometry_clear {
            Err(CastError::Obstructed)
        } else {
            Ok(())
        }
    }
    fn stamp(&mut self) -> Result<u64, CastError> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(CastError::SequenceExhausted)?;
        self.outstanding = self.sequence;
        Ok(self.sequence)
    }
    fn finish(&mut self, error: Option<CastError>) -> CastSignal {
        let cast = self.prepared.as_ref().map_or(0, |p| p.id);
        self.stage = CastStage::Finished;
        self.prepared = None;
        CastSignal::Finished { cast, error }
    }
    fn displaced(&self, o: CastObservation) -> bool {
        o.cell != self.cell || (o.position - self.origin).length_squared() >= 36.0
    }
    fn advance(&mut self, now: f64, o: CastObservation) -> Result<CastSignal, CastError> {
        let p = self
            .prepared
            .as_ref()
            .ok_or(CastError::InvalidPreparation)?;
        let needs_heading = self.index == 0 || self.index == p.gestures.len();
        if !instant(p) && needs_heading && o.heading_to_target.is_some_and(|a| a > 45.0) {
            let target = p.target.ok_or(CastError::InvalidTarget)?;
            let cast = p.id;
            let sequence = self.stamp()?;
            if self.stage != CastStage::Turning {
                self.timeout = now + 9999.0;
            }
            self.stage = CastStage::Turning;
            return Ok(CastSignal::Turn {
                cast,
                target,
                sequence,
                maximum_angle_degrees: 45.0,
            });
        }
        if let Some(gesture) = p.gestures.get(self.index).copied() {
            let cast = p.id;
            let sequence = self.stamp()?;
            self.stage = CastStage::Gesture;
            self.timeout = now + 4.0;
            self.next_cast = now + gesture.minimum_seconds * 0.5;
            return Ok(CastSignal::Gesture {
                cast,
                motion: gesture.motion,
                index: self.index,
                sequence,
                speed: 2.0,
                stop_movement: true,
            });
        }
        if let Err(error) = Self::eligibility(o, instant(p)) {
            return Ok(self.finish(Some(error)));
        }
        if p.fast_resistable_pk_spell && !self.release_recorded && now < self.streak_until {
            return Ok(self.finish(Some(CastError::StreakCooldown)));
        }
        let signal = CastSignal::Release {
            cast: p.id,
            spell: p.spell,
            target: p.target,
        };
        self.stage = CastStage::Release;
        Ok(signal)
    }
    pub fn update(&mut self, now: f64, o: CastObservation) -> Result<CastSignal, CastError> {
        self.validate_time(now)?;
        Self::validate_observation(o)?;
        self.last_now = now;
        if !self.active() {
            return Ok(CastSignal::Waiting);
        }
        let source_instant = self.prepared.as_ref().is_some_and(instant);
        if !source_instant && !o.alive {
            return Ok(self.finish(Some(CastError::Dead)));
        }
        if !source_instant && o.in_portal {
            return Ok(self.finish(Some(CastError::PortalSpace)));
        }
        if o.peace_mode && self.prepared.as_ref().is_some_and(|p| p.player) {
            let cast = self.prepared.as_ref().expect("active cast").id;
            self.finish(None);
            return Ok(CastSignal::PeaceFizzle {
                cast,
                mana_cost: 5,
                intensity: f32::from_bits(0x3f0af0a2),
            });
        }
        // A pending durable release is retained until its owner resolves it.
        if self.stage == CastStage::Release {
            return Ok(CastSignal::Waiting);
        }
        if now >= self.timeout {
            return Ok(self.finish(Some(CastError::TimedOut)));
        }
        if self.turned && now >= self.next_turn && self.displaced(o) {
            return Ok(self.movement_fizzle());
        }
        if self.stage == CastStage::Turning {
            if !o.turning_to_target && o.heading_to_target.is_none_or(|a| a <= 45.0) {
                return self.advance(now, o);
            }
            if !o.turning_to_target && !o.manual_turning && now >= self.next_turn {
                self.turned = true;
                self.next_turn = now + 1.25;
                return self.advance(now, o);
            }
        }
        Ok(CastSignal::Waiting)
    }
    pub fn motion_done(
        &mut self,
        cast: u64,
        sequence: u64,
        motion: u32,
        success: bool,
        now: f64,
        o: CastObservation,
    ) -> Result<CastSignal, CastError> {
        self.validate_time(now)?;
        Self::validate_observation(o)?;
        let p = self.prepared.as_ref().ok_or(CastError::StaleCompletion)?;
        if self.stage != CastStage::Gesture
            || p.id != cast
            || self.outstanding != sequence
            || p.gestures
                .get(self.index)
                .is_none_or(|g| g.motion != motion)
        {
            return Err(CastError::StaleCompletion);
        }
        self.last_now = now;
        if now >= self.timeout {
            return Ok(self.finish(Some(CastError::TimedOut)));
        }
        if !o.alive {
            return Ok(self.finish(Some(CastError::Dead)));
        }
        if !success {
            return Ok(self.finish(Some(CastError::MotionFailed)));
        }
        self.index += 1;
        self.advance(now, o)
    }
    /// Deferred durable resources do not authorize a stale target or heading.
    /// Re-enter the same final turn/release path without rerolling cast resources.
    pub fn revalidate_release(
        &mut self,
        now: f64,
        observation: CastObservation,
    ) -> Result<CastSignal, CastError> {
        self.validate_time(now)?;
        Self::validate_observation(observation)?;
        if self.stage != CastStage::Release {
            return Err(CastError::NotReleasing);
        }
        if let Err(error) =
            Self::eligibility(observation, self.prepared.as_ref().is_some_and(instant))
        {
            return Ok(self.finish(Some(error)));
        }
        self.last_now = now;
        self.advance(now, observation)
    }
    pub fn movement_disrupted(&self, observation: CastObservation) -> bool {
        self.displaced(observation)
    }
    pub fn movement_fizzle_at_release(
        &mut self,
        cast: u64,
        now: f64,
    ) -> Result<CastSignal, CastError> {
        self.record_release_attempt(cast, now)?;
        Ok(self.movement_fizzle())
    }
    fn movement_fizzle(&mut self) -> CastSignal {
        let cast = self.prepared.as_ref().map_or(0, |p| p.id);
        self.finish(None);
        CastSignal::MovementFizzle {
            cast,
            mana_cost: 5,
            intensity: f32::from_bits(0x3f0af0a2),
        }
    }
    pub fn resolve_release(
        &mut self,
        cast: u64,
        now: f64,
        result: Result<(), CastError>,
    ) -> Result<CastSignal, CastError> {
        self.validate_time(now)?;
        if self.stage != CastStage::Release || self.prepared.as_ref().is_none_or(|p| p.id != cast) {
            return Err(CastError::NotReleasing);
        }
        self.last_now = now;
        self.record_release_attempt(cast, now)?;
        Ok(self.finish(result.err()))
    }
    /// GDLE advances NEXT_SPELLCAST_TIMESTAMP after LaunchSpellEffect even when
    /// that attempted release returns an error. A durable retry is not a new launch.
    pub fn record_release_attempt(&mut self, cast: u64, now: f64) -> Result<(), CastError> {
        self.validate_time(now)?;
        if self.stage != CastStage::Release || self.prepared.as_ref().is_none_or(|p| p.id != cast) {
            return Err(CastError::NotReleasing);
        }
        if !self.release_recorded {
            if self
                .prepared
                .as_ref()
                .is_some_and(|p| p.fast_resistable_pk_spell)
            {
                self.streak_until = now + 2.0;
            }
            self.release_recorded = true;
        }
        Ok(())
    }
    pub fn cancel(&mut self) -> CastSignal {
        self.finish(Some(CastError::MotionFailed))
    }
}

/// GDLE CastSpellInstant bypasses CreatureBeginCast/player source eligibility;
/// its source object must still exist and every target check remains in force.
fn instant(prepared: &CastPreparation) -> bool {
    !prepared.player && !prepared.uses_mana && prepared.gestures.is_empty()
}
