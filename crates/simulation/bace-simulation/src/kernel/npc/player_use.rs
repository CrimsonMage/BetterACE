//! ACE Player.HandleActionUseItem approaches a world NPC before activation.
//! Movement and the arrival check stay on the authoritative simulation owner.
use super::Kernel;
use bace_gameplay_api::visibility::{AcceptedMoveParameters, ServerMovementGoal};
use bace_gameplay_api::{ActionContext, NpcFailure};
use bace_geometry::Vec3;
use bace_motion::{MotionIntent, TurnControl, TurnIntent, heading_delta};
use bace_types::EntityId;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
pub(in crate::kernel) struct Approach {
    context: ActionContext,
    source: EntityId,
    event: [u8; 16],
    operation: u64,
    control: Option<TurnControl>,
    started_tick: u64,
    cancelled: bool,
}

impl Kernel {
    pub(super) fn begin_player_npc_use(
        &mut self,
        context: ActionContext,
        source: EntityId,
        event: [u8; 16],
        operation: u64,
    ) -> Result<bool, NpcFailure> {
        if self.npc_player_uses.contains_key(&context.actor)
            || self
                .npc_player_uses
                .values()
                .any(|use_| use_.source == source)
        {
            return Err(NpcFailure::DurabilityPending);
        }
        if self
            .npcs
            .player_use_in_range(source, context.actor, &self.world)?
        {
            return self.activate_player_npc_use(context.actor, source, event, operation);
        }
        // The signed portal threshold uses a separate MoveToPosition chain.
        if self
            .npcs
            .player_use_radius(source)
            .ok_or(NpcFailure::MissingActor)?
            < 0.0
        {
            return Err(NpcFailure::InvalidInput);
        }
        if self.npc_player_uses.len() >= self.capacity {
            return Err(NpcFailure::Capacity);
        }
        let mut approach = Approach {
            context,
            source,
            event,
            operation,
            control: None,
            started_tick: self.tick,
            cancelled: false,
        };
        if let Err(error) = self.advance_player_npc_approach(&mut approach) {
            self.finish_player_npc_approach(&approach);
            return Err(error);
        }
        self.npc_player_uses.insert(context.actor, approach);
        Ok(false)
    }

    pub fn poll_player_npc_use(
        &mut self,
        actor: EntityId,
        source: EntityId,
    ) -> Result<bool, NpcFailure> {
        let mut approach = self
            .npc_player_uses
            .remove(&actor)
            .ok_or(NpcFailure::Conflict)?;
        if approach.source != source {
            self.npc_player_uses.insert(actor, approach);
            return Err(NpcFailure::Conflict);
        }
        // ACE GameActionMoveToState stops the active MoveTo chain. The later
        // TryUseItem(item, false) completes the Use action without activating
        // the target, so retain this result until the pending PollUse receipt.
        if approach.cancelled {
            return Ok(false);
        }
        if self.tick.saturating_sub(approach.started_tick) >= 450
            || self.characters.get(actor).is_none()
            || self.world.is_in_portal_transit(actor)
        {
            self.finish_player_npc_approach(&approach);
            return Err(NpcFailure::Cancelled);
        }
        let in_range = match self.npcs.player_use_in_range(source, actor, &self.world) {
            Ok(value) => value,
            Err(error) => {
                self.finish_player_npc_approach(&approach);
                return Err(error);
            }
        };
        if in_range {
            self.finish_player_npc_approach(&approach);
            return self.activate_player_npc_use(
                approach.context.actor,
                source,
                approach.event,
                approach.operation,
            );
        }
        if let Err(error) = self.advance_player_npc_approach(&mut approach) {
            self.finish_player_npc_approach(&approach);
            return Err(error);
        }
        self.npc_player_uses.insert(actor, approach);
        Ok(false)
    }

    fn activate_player_npc_use(
        &mut self,
        actor: EntityId,
        source: EntityId,
        event: [u8; 16],
        operation: u64,
    ) -> Result<bool, NpcFailure> {
        self.start_npc_emote(
            source,
            Some(actor),
            bace_emotes::NativeTrigger {
                category: 7,
                ..Default::default()
            },
            event,
            operation,
            true,
        )
        .map_err(|error| match error {
            bace_emotes::NativeError::Owner(error) => error,
            bace_emotes::NativeError::Busy => NpcFailure::DurabilityPending,
            _ => NpcFailure::InvalidInput,
        })
    }

    pub(in crate::kernel) fn finish_player_npc_approach(&mut self, approach: &Approach) {
        if let Some(control) = approach.control {
            self.world
                .finish_server_move(approach.context.actor, control);
            if let Ok(body) = self.world.body_mut(approach.context.actor) {
                body.finish_server_turn(control);
            }
            self.world
                .clear_movement_goal(approach.context.actor, control);
        }
    }

    pub(in crate::kernel) fn cancel_player_npc_approach(&mut self, actor: EntityId) {
        let Some(approach) = self.npc_player_uses.get_mut(&actor) else {
            return;
        };
        if approach.cancelled {
            return;
        }
        approach.cancelled = true;
        let approach = *approach;
        self.finish_player_npc_approach(&approach);
    }

    fn advance_player_npc_approach(&mut self, approach: &mut Approach) -> Result<(), NpcFailure> {
        let actor = approach.context.actor;
        let (cell, accepted) = self
            .world
            .actor_state(actor)
            .map_err(|_| NpcFailure::MissingActor)?;
        let (target_cell, target) = self
            .world
            .actor_state(approach.source)
            .map_err(|_| NpcFailure::MissingActor)?;
        let mut target_position = target.position();
        if cell != target_cell {
            target_position = target_position
                + self
                    .world
                    .geometry()
                    .ok_or(NpcFailure::MissingContent)?
                    .frame_offset(cell.0, target_cell.0)
                    .map_err(|_| NpcFailure::MissingContent)?;
        }
        let offset = target_position - accepted.position();
        let heading = (-offset.x).atan2(offset.y);
        let delta = heading_delta(accepted.heading_radians(), heading)
            .map_err(|_| NpcFailure::InvalidInput)?;
        let turn = TurnIntent::new((delta / 0.25).clamp(-1.0, 1.0))
            .map_err(|_| NpcFailure::InvalidInput)?;
        let direction = if delta.abs() < 0.5 {
            offset * (1.0 / offset.length_squared().sqrt().max(0.001))
        } else {
            Vec3::ZERO
        };
        let intent = MotionIntent::new(direction, false).map_err(|_| NpcFailure::InvalidInput)?;
        let control = match approach.control {
            Some(control) => control,
            None => self
                .world
                .next_server_control(actor)
                .map_err(|_| NpcFailure::Capacity)?,
        };
        if approach.control.is_some() {
            self.world
                .continue_server_move(actor, accepted.epoch(), control, intent)
                .map_err(|_| NpcFailure::InvalidInput)?;
            self.world
                .body_mut(actor)
                .map_err(|_| NpcFailure::MissingActor)?
                .continue_server_turn(accepted.epoch(), control, turn)
                .map_err(|_| NpcFailure::InvalidInput)?;
        } else {
            self.world
                .begin_server_move(actor, accepted.epoch(), control, intent)
                .map_err(|_| NpcFailure::MissingContent)?;
            if self
                .world
                .body_mut(actor)
                .map_err(|_| NpcFailure::MissingActor)?
                .begin_server_turn(accepted.epoch(), control, turn)
                .is_err()
            {
                self.world.finish_server_move(actor, control);
                return Err(NpcFailure::InvalidInput);
            }
            approach.control = Some(control);
        }
        let run_rate = self
            .world
            .body(actor)
            .ok()
            .and_then(|body| body.locomotion_run_rate())
            .filter(|rate| (0.0..=20.0).contains(rate))
            .unwrap_or(1.0);
        let use_radius = self
            .npcs
            .player_use_radius(approach.source)
            .ok_or(NpcFailure::MissingActor)?;
        self.world
            .bind_movement_goal(
                actor,
                control,
                ServerMovementGoal::MoveToObject {
                    target: approach.source,
                    parameters: AcceptedMoveParameters {
                        flags: 0x1f09eff0,
                        distance_to_object: use_radius.max(0.0),
                        min_distance: 0.1,
                        fail_distance: 15.0,
                        speed: 1.0,
                        walk_run_threshold: 15.0,
                        desired_heading: 0.0,
                    },
                    run_rate,
                },
            )
            .map_err(|_| NpcFailure::InvalidInput)
    }
}
