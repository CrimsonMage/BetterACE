//! Bounded immutable style assets beside the one authoritative Body interpreter.
use super::*;
use bace_motion::AnimatedLocomotion;
use std::sync::Arc;
impl World {
    pub fn validate_locomotion_styles(
        &self,
        actor: EntityId,
        styles: &[Arc<AnimatedLocomotion>],
    ) -> Result<(), WorldError> {
        if styles.is_empty()
            || styles.len() > 4
            || (!self.locomotion_style_actors.contains(&actor)
                && self.locomotion_style_actors.len() == 4096)
        {
            return Err(WorldError::InvalidMotion);
        }
        for (index, style) in styles.iter().enumerate() {
            if style.profile.style == 0
                || styles[..index]
                    .iter()
                    .any(|s| s.profile.style == style.profile.style)
                || self
                    .locomotion_styles
                    .contains_key(&(actor, style.profile.style))
            {
                return Err(WorldError::InvalidMotion);
            }
            style
                .profile
                .interpret(bace_motion::LocomotionControls::default(), 1.0)
                .map_err(|_| WorldError::InvalidMotion)?;
        }
        Ok(())
    }
    pub fn register_locomotion_styles(
        &mut self,
        actor: EntityId,
        styles: Vec<Arc<AnimatedLocomotion>>,
    ) -> Result<(), WorldError> {
        self.body(actor)?;
        self.validate_locomotion_styles(actor, &styles)?;
        self.locomotion_style_actors.insert(actor);
        for style in styles {
            self.locomotion_styles
                .insert((actor, style.profile.style), style);
        }
        Ok(())
    }
    pub fn locomotion_style(
        &self,
        actor: EntityId,
        style: u32,
    ) -> Option<&Arc<AnimatedLocomotion>> {
        self.locomotion_styles.get(&(actor, style))
    }
}
impl World {
    pub fn validate_replacement_locomotion_styles(
        &self,
        actor: EntityId,
        styles: &[Arc<AnimatedLocomotion>],
    ) -> Result<(), WorldError> {
        if styles.is_empty()
            || styles.len() > 4
            || (!self.locomotion_style_actors.contains(&actor)
                && self.locomotion_style_actors.len() == 4096)
        {
            return Err(WorldError::InvalidMotion);
        }
        for (index, style) in styles.iter().enumerate() {
            if style.profile.style == 0
                || styles[..index]
                    .iter()
                    .any(|s| s.profile.style == style.profile.style)
            {
                return Err(WorldError::InvalidMotion);
            }
            style
                .profile
                .interpret(bace_motion::LocomotionControls::default(), 1.0)
                .map_err(|_| WorldError::InvalidMotion)?;
        }
        Ok(())
    }
    /// Called only after the matching equipment receipt. Body retains its actual
    /// old style Arc until the authored transition selects a new style.
    pub fn replace_locomotion_styles(
        &mut self,
        actor: EntityId,
        styles: Vec<Arc<AnimatedLocomotion>>,
    ) -> Result<(), WorldError> {
        self.body(actor)?;
        self.validate_replacement_locomotion_styles(actor, &styles)?;
        self.locomotion_styles.retain(|(id, _), _| *id != actor);
        self.locomotion_style_actors.insert(actor);
        for style in styles {
            self.locomotion_styles
                .insert((actor, style.profile.style), style);
        }
        Ok(())
    }
}

impl World {
    /// Four slots are reserved for every registered actor, so an immutable
    /// equipment replacement preflight cannot lose capacity during DB latency.
    pub fn remaining_locomotion_style_actors(&self) -> usize {
        4096 - self.locomotion_style_actors.len()
    }
}

impl World {
    /// Explicit lifecycle-owner reset after teleport/respawn. It cannot replace
    /// an active same-epoch animation or accept a packet-supplied style asset.
    pub fn reset_locomotion_style(
        &mut self,
        actor: EntityId,
        epoch: u16,
        style: u32,
        run_rate: f32,
    ) -> Result<(), WorldError> {
        let body = self.body(actor)?;
        if body.accepted().epoch() != epoch
            || self
                .motions
                .get(&actor)
                .is_some_and(|m| m.epoch == epoch && !m.playback.cursor().completed)
        {
            return Err(WorldError::InvalidMotion);
        }
        let profile = self
            .locomotion_style(actor, style)
            .cloned()
            .ok_or(WorldError::InvalidMotion)?;
        profile
            .profile
            .interpret(bace_motion::LocomotionControls::default(), run_rate)
            .map_err(|_| WorldError::InvalidMotion)?;
        body.validate_animated_style(&profile, true)
            .map_err(|_| WorldError::InvalidMotion)?;
        let body = self.body_mut(actor)?;
        body.adopt_animated_style(profile.clone(), true)
            .expect("preflighted style");
        body.refresh_locomotion(&profile.profile, run_rate)
            .expect("preflighted run rate");
        Ok(())
    }
}
