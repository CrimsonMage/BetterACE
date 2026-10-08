//! Authenticated raw controls select only cold admitted motion/capability assets.
//! Client positions, velocity, contact and time never mutate accepted state.
use super::*;
use bace_entity::{EntityVital, VitalMutation};
use bace_gameplay_api::locomotion::*;
use bace_motion::{RawLocomotion, interpret_raw_controls};
impl Kernel {
    pub fn apply_locomotion(&mut self, command: LocomotionCommand) -> LocomotionOutcome {
        let result = self.apply_locomotion_inner(command);
        LocomotionOutcome {
            correlation: command.correlation,
            context: command.context,
            tick: self.tick,
            result,
        }
    }
    fn apply_locomotion_inner(
        &mut self,
        command: LocomotionCommand,
    ) -> Result<LocomotionAccepted, LocomotionRejection> {
        use LocomotionRejection as E;
        let context = command.context;
        let actor = context.actor;
        if command.correlation == 0 {
            return Err(E::Invalid);
        }
        self.characters
            .authorize(context, self.world.body(actor).is_ok())
            .map_err(|_| E::NotBound)?;
        if !self.characters.entered(actor) {
            return Err(E::NotBound);
        }
        if self.characters.reserved(actor)
            || self.inventory.reserved(actor)
            || self.npcs.reserved(actor)
            || self.housing.reserved(actor)
            || self.world.retirement_held(actor)
            || self.world.is_in_portal_transit(actor)
        {
            return Err(E::Busy);
        }
        let body = self.world.body(actor).map_err(|_| E::NotBound)?;
        if body.accepted().epoch() != command.teleport {
            return Err(E::Stale);
        }
        if body.collision_shape().is_none()
            || self.world.combatant(actor).is_none_or(|c| c.health() == 0)
        {
            return Err(E::Invalid);
        }
        let mut stamina = None;
        match command.request {
            LocomotionRequest::ObservePosition => {}
            LocomotionRequest::State(raw) => {
                let controls = interpret_raw_controls(RawLocomotion {
                    current_hold: raw.current_hold,
                    forward: raw.forward,
                    sidestep: raw.sidestep,
                    turn: raw.turn,
                })
                .map_err(|_| E::Invalid)?;
                let current = self
                    .world
                    .source_motion_state(actor)
                    .ok_or(E::MissingAssets)?;
                if raw.style != current.style {
                    return Err(E::Stale);
                }
                let profile = self
                    .world
                    .locomotion_style(actor, current.style)
                    .cloned()
                    .ok_or(E::MissingAssets)?;
                let run = self.live_run_rate(actor)?;
                self.world
                    .body_mut(actor)
                    .map_err(|_| E::NotBound)?
                    .submit_animated_axes(
                        command.teleport,
                        context.sequence,
                        profile,
                        controls,
                        run,
                    )
                    .map_err(physics_error)?;
            }
            LocomotionRequest::Jump { extent } => {
                if !extent.is_finite() {
                    return Err(E::Invalid);
                }
                let (strength, skill, augment, _) = self.locomotion_attributes(actor, 22)?;
                let burden = self.actor_inventory_burden(actor).map_err(|_| E::Busy)?;
                let burden = bace_character::encumbrance(strength, augment, burden)
                    .map_err(|_| E::Invalid)?
                    .ratio;
                let before = self
                    .world
                    .vital(actor, EntityVital::Stamina)
                    .map_err(|_| E::MissingAssets)?
                    .current;
                let proposal = bace_character::jump_proposal(bace_character::JumpInput {
                    skill: if before == 0 { 0 } else { skill },
                    burden,
                    // ACE Physics/Common/WeenieObject.InqJumpVelocity passes
                    // literal1.0; visual DefaultScale only affects RunRate.
                    scale: 1.0,
                    extent,
                    stamina: before,
                    pk_timer_active: self.pk_timer_active(actor),
                })
                .map_err(|e| {
                    if e == bace_character::LocomotionError::TooTired {
                        E::TooTired
                    } else {
                        E::Invalid
                    }
                })?;
                self.world
                    .body(actor)
                    .map_err(|_| E::NotBound)?
                    .validate_jump_command(command.teleport, context.sequence, proposal.height)
                    .map_err(physics_error)?;
                let change = VitalMutation {
                    actor,
                    vital: EntityVital::Stamina,
                    before,
                    after: proposal.remaining_stamina,
                };
                self.world
                    .validate_vital_batch(&[change], None)
                    .map_err(|_| E::Busy)?;
                self.world
                    .body_mut(actor)
                    .map_err(|_| E::NotBound)?
                    .authorize_jump_command(command.teleport, context.sequence, proposal.height)
                    .expect("same-owner preflighted jump");
                self.world
                    .apply_vital_batch(&[change], None)
                    .expect("same-owner preflighted stamina");
                stamina = Some((before, proposal.remaining_stamina));
            }
        }
        Ok(LocomotionAccepted {
            vital_revision: self.world.combatant(actor).ok_or(E::NotBound)?.revision(),
            view: self
                .world
                .accepted_object_view(actor)
                .map_err(|_| E::MissingAssets)?,
            stamina,
        })
    }
    fn locomotion_attributes(
        &mut self,
        actor: EntityId,
        skill: u32,
    ) -> Result<(u32, u32, u32, f32), LocomotionRejection> {
        use LocomotionRejection as E;
        self.refresh_character_skills(actor).map_err(|_| E::Busy)?;
        let profile = self
            .combat
            .physical_profile(actor)
            .ok_or(E::MissingAssets)?;
        let skill = profile
            .skills
            .iter()
            .find(|(id, _)| *id == skill)
            .map(|(_, value)| value.current)
            .ok_or(E::MissingAssets)?;
        let source = self
            .combat
            .physical_refresh_source(actor)
            .ok_or(E::MissingAssets)?;
        let augment = source
            .weenie
            .properties
            .ints
            .iter()
            .find(|p| p.id == 230)
            .map_or(0, |p| p.value.max(0) as u32);
        let scale = source
            .weenie
            .properties
            .floats
            .iter()
            .find(|p| p.id == 39)
            .map_or(1.0, |p| p.value) as f32;
        Ok((profile.strength, skill, augment, scale))
    }
    fn live_run_rate(&mut self, actor: EntityId) -> Result<f32, LocomotionRejection> {
        use LocomotionRejection as E;
        let (strength, skill, augment, scale) = self.locomotion_attributes(actor, 24)?;
        let burden = self.actor_inventory_burden(actor).map_err(|_| E::Busy)?;
        let burden = bace_character::encumbrance(strength, augment, burden)
            .map_err(|_| E::Invalid)?
            .ratio;
        let exhausted = self
            .world
            .vital(actor, EntityVital::Stamina)
            .map_err(|_| E::MissingAssets)?
            .current
            == 0;
        bace_character::run_rate(bace_character::RunInput {
            skill,
            burden,
            scale,
            exhausted,
        })
        .map_err(|_| E::Invalid)
    }
    pub fn refresh_locomotion_actor(&mut self, actor: EntityId) -> Result<(), LocomotionRejection> {
        let rate = self.live_run_rate(actor)?;
        let profile = self
            .world
            .body(actor)
            .map_err(|_| LocomotionRejection::NotBound)?
            .animated_locomotion()
            .cloned()
            .ok_or(LocomotionRejection::MissingAssets)?;
        self.world
            .body_mut(actor)
            .map_err(|_| LocomotionRejection::NotBound)?
            .refresh_locomotion(&profile.profile, rate)
            .map_err(physics_error)
    }
}
fn physics_error(error: bace_physics::PhysicsError) -> LocomotionRejection {
    match error {
        bace_physics::PhysicsError::StaleEpoch | bace_physics::PhysicsError::StaleSequence => {
            LocomotionRejection::Stale
        }
        _ => LocomotionRejection::Invalid,
    }
}
