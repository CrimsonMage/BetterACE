//! Source ClapHands completes on the existing authoritative motion owner before
//! quoting or drawing. No wall clock, synthetic animation or second client action.
use super::*;
use bace_crafting::CraftError as E;
use bace_motion::{MotionDomain, MotionExecutionEvent, MotionToken, PreparedMotionChain};
use std::sync::Arc;
impl Kernel {
    pub(super) fn begin_tinker_motion(
        &mut self,
        correlation: u64,
        context: ActionContext,
        input: Box<crate::TinkerCommandInput>,
        motion: Arc<PreparedMotionChain>,
        quote: bool,
        lifetime: u64,
    ) -> Result<crate::CraftingResult, E> {
        if correlation == 0
            || lifetime == 0
            || lifetime > 1800
            || self.crafting.animations.len() >= 64
            || self.crafting.animations.contains_key(&context.actor)
        {
            return Err(E::Busy);
        }
        super::crafting::validate_actor_recipe(&input.recipe)?;
        let source = motion.source_transition().ok_or(E::InvalidState)?;
        if motion.motion != 0x1300007e || source.before.style != 0x8000003d || motion.speed != 1.0 {
            return Err(E::InvalidState);
        }
        self.authorize_crafting(context, input.context.actor, input.context.actor_revision)?;
        self.crafting.quote(
            crate::crafting::QuoteInput {
                context: &input.context,
                source: &input.source,
                target: &input.target,
                recipe: &input.recipe,
                now: self.tick,
                lifetime,
            },
            &self.inventory,
        )?;
        let registries = match self.reserve_crafting_registries(context.actor) {
            Ok(v) => v,
            Err(e) => {
                self.crafting.cancel_quote(context.actor);
                return Err(e);
            }
        };
        if self
            .characters
            .get(context.actor)
            .is_none_or(|c| c.revision() != input.context.actor_revision)
            || self
                .characters
                .reserve_crafting(context.actor, correlation)
                .is_err()
        {
            self.release_crafting_registries(&registries)?;
            self.crafting.cancel_quote(context.actor);
            return Err(E::StaleConfirmation);
        }
        let token = MotionToken {
            domain: MotionDomain::Crafting,
            owner: correlation,
            sequence: 1,
        };
        if self
            .world
            .begin_motion(context.actor, token, motion)
            .is_err()
        {
            self.characters
                .release_crafting(context.actor, correlation)
                .map_err(|_| E::InvalidState)?;
            self.release_crafting_registries(&registries)?;
            self.crafting.cancel_quote(context.actor);
            return Err(E::Busy);
        }
        self.crafting.animations.insert(
            context.actor,
            crate::crafting::CraftingAnimation {
                correlation,
                context,
                input,
                token,
                quote,
                lifetime,
                registries,
            },
        );
        Ok(crate::CraftingResult::Animating {
            actor: context.actor,
            style: source.before.style,
            command: 0x1300007e,
        })
    }
    pub(super) fn step_crafting(&mut self) -> Result<(), SimulationError> {
        self.crafting.expire_quotes(self.tick);
        self.crafting.animation_scratch.clear();
        self.crafting
            .animation_scratch
            .extend(self.crafting.animations.keys().copied());
        for index in 0..self.crafting.animation_scratch.len() {
            if self.crafting_outcomes.len() >= self.outcome_capacity {
                break;
            }
            let actor = self.crafting.animation_scratch[index];
            let token = self
                .crafting
                .animations
                .get(&actor)
                .expect("owner scratch")
                .token;
            let mut completion = None;
            for _ in 0..128 {
                let Some(event) = self.world.take_motion_event_matching(actor, token) else {
                    break;
                };
                match event.event {
                    MotionExecutionEvent::Completed => {
                        completion = Some(true);
                        break;
                    }
                    MotionExecutionEvent::Cancelled => {
                        completion = Some(false);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(completed) = completion else {
                continue;
            };
            self.world
                .end_crafting_motion(actor, token.owner)
                .map_err(|_| SimulationError::InvalidCommand)?;
            let pending = self
                .crafting
                .animations
                .remove(&actor)
                .expect("retained animation");
            self.characters
                .release_crafting(actor, pending.correlation)
                .map_err(|_| SimulationError::InvalidCommand)?;
            let valid = completed
                && self
                    .world
                    .combatant(actor)
                    .is_some_and(|c| c.mode() == 1 && c.health() > 0);
            let result = if valid {
                let input = &pending.input;
                self.crafting.quote(
                    crate::crafting::QuoteInput {
                        context: &input.context,
                        source: &input.source,
                        target: &input.target,
                        recipe: &input.recipe,
                        now: self.tick,
                        lifetime: pending.lifetime,
                    },
                    &self.inventory,
                )
            } else {
                Err(E::InvalidState)
            };
            let result = match result {
                Ok(chance) if pending.quote => {
                    self.release_crafting_registries(&pending.registries)
                        .map_err(|_| SimulationError::InvalidCommand)?;
                    Ok(crate::CraftingResult::Quoted(chance))
                }
                Ok(_) => self
                    .confirm_tinker_reserved(
                        pending.context,
                        &pending.input.context,
                        &pending.input.source,
                        &pending.input.target,
                        &pending.input.recipe,
                        pending.registries,
                    )
                    .map(crate::CraftingResult::Pending),
                Err(error) => {
                    self.release_crafting_registries(&pending.registries)
                        .map_err(|_| SimulationError::InvalidCommand)?;
                    self.crafting.cancel_quote(actor);
                    Err(error)
                }
            };
            self.crafting_outcomes.push_back(crate::CraftingOutcome {
                correlation: pending.correlation,
                result,
            });
        }
        Ok(())
    }
}
