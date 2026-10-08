//! Explicit source rate roles permit bounded re-preparation after live Quickness
//! changes. Numeric equality of old rates never determines their responsibility.
use crate::{MotionPhysics, PreparedMotionChain, TimelineError};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionRate {
    Action,
    Current,
    Unit,
    NegativeAction,
}
impl MotionRate {
    pub fn value(self, current: f32, action: f32) -> f32 {
        match self {
            Self::Action => action,
            Self::Current => current,
            Self::Unit => 1.0,
            Self::NegativeAction => -action,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionClipRate {
    pub base: f32,
    pub role: MotionRate,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MotionRateModel {
    pub clears_modifiers: bool,
    pub clips: Vec<MotionClipRate>,
    pub cycle_rate: MotionRate,
    pub cycle_physics: MotionPhysics,
    pub modifiers: Vec<(MotionPhysics, f32)>,
    pub scale: f32,
}
impl MotionRateModel {
    fn physics(&self, current: f32, action: f32) -> MotionPhysics {
        let speed = self.cycle_rate.value(current, action);
        let mut physics = MotionPhysics {
            velocity: self.cycle_physics.velocity * speed,
            omega: self.cycle_physics.omega * speed,
        };
        for (modifier, rate) in &self.modifiers {
            physics.velocity = physics.velocity + modifier.velocity * *rate;
            physics.omega = physics.omega + modifier.omega * *rate;
        }
        physics.velocity = physics.velocity * self.scale;
        physics
    }
}
impl PreparedMotionChain {
    pub fn with_rate_model(mut self, model: MotionRateModel) -> Result<Self, TimelineError> {
        let source = self
            .source_transition()
            .ok_or(TimelineError::InvalidRange)?;
        if model.clips.len() != self.clips().len()
            || model.modifiers.len() > 16
            || !model.scale.is_finite()
            || !(0.001..=100.0).contains(&model.scale)
            || model.physics(source.before.speed, self.speed) != self.physics
            || model.clips.iter().zip(self.clips()).any(|(rate, clip)| {
                !rate.base.is_finite()
                    || (rate.base * rate.role.value(source.before.speed, self.speed)).to_bits()
                        != clip.framerate.to_bits()
            })
        {
            return Err(TimelineError::InvalidRate);
        }
        self.rate_model = Some(Arc::new(model));
        Ok(self)
    }
    pub fn retime_action(&self, speed: f32) -> Result<Self, TimelineError> {
        let source = self
            .source_transition()
            .ok_or(TimelineError::InvalidRange)?;
        self.retime(source.before.speed, speed)
    }
    /// Rebind only authored modifier physics to the currently accepted controls.
    /// Clip selection, hooks, action rates and before/after substates stay intact.
    pub fn with_locomotion_modifiers(
        &self,
        profile: &crate::LocomotionProfile,
        drive: crate::MotionDrive,
    ) -> Result<Self, TimelineError> {
        let source = self
            .source_transition()
            .ok_or(TimelineError::InvalidRange)?;
        if profile.style != source.after.style {
            return Err(TimelineError::InvalidRange);
        }
        let mut model = (**self.rate_model.as_ref().ok_or(TimelineError::InvalidRate)?).clone();
        model.modifiers.clear();
        if !model.clears_modifiers {
            for (mut physics, rate) in [
                (profile.sidestep, drive.side_rate),
                (profile.turn, drive.turn_rate),
            ] {
                if !rate.is_finite() || rate.abs() > 20.0 {
                    return Err(TimelineError::InvalidRate);
                }
                if rate != 0.0 {
                    physics.velocity = physics.velocity * (1.0 / model.scale);
                    model.modifiers.push((physics, rate));
                }
            }
        }
        let physics = model.physics(source.before.speed, self.speed);
        let mut result = Self::prepare(
            self.clips().to_vec(),
            self.first_cyclic(),
            self.completion_clips(),
            physics,
            self.motion,
            self.speed,
        )?
        .with_source_transition(source)?;
        result.rate_model = Some(Arc::new(model));
        if let Some(stop) = self.stop_chain() {
            result = result
                .with_stop_chain(Arc::new(stop.with_locomotion_modifiers(profile, drive)?))?;
        }
        Ok(result)
    }
    pub fn clears_style_modifiers(&self) -> bool {
        self.rate_model.as_ref().is_some_and(|m| m.clears_modifiers)
    }
    pub fn retime_current_action(&self, current: f32, action: f32) -> Result<Self, TimelineError> {
        self.retime(current, action)
    }
    fn retime(&self, current: f32, action: f32) -> Result<Self, TimelineError> {
        let mut transition = self
            .source_transition()
            .ok_or(TimelineError::InvalidRange)?;
        if current == transition.before.speed && action == self.speed {
            return Ok(self.clone());
        }
        // Preserve both rate signs: get_link changes its lookup direction at zero.
        if current == 0.0
            || action <= 0.0
            || !current.is_finite()
            || !action.is_finite()
            || current.abs() > 20.0
            || action > 20.0
            || current.is_sign_negative() != transition.before.speed.is_sign_negative()
            || self.speed <= 0.0
            || self.continues_cycle()
        {
            return Err(TimelineError::InvalidRate);
        }
        let model = self.rate_model.as_ref().ok_or(TimelineError::InvalidRate)?;
        let mut clips = self.clips().to_vec();
        for (clip, rate) in clips.iter_mut().zip(&model.clips) {
            clip.framerate = rate.base * rate.role.value(current, action);
        }
        transition.before.speed = current;
        transition.after.speed = model.cycle_rate.value(current, action);
        let mut result = Self::prepare(
            clips,
            self.first_cyclic(),
            self.completion_clips(),
            model.physics(current, action),
            self.motion,
            action,
        )?
        .with_source_transition(transition)?;
        result.rate_model = Some(model.clone());
        if let Some(stop) = self.stop_chain() {
            result = result
                .with_stop_chain(Arc::new(stop.retime(transition.after.speed, stop.speed)?))?;
        }
        Ok(result)
    }
}
