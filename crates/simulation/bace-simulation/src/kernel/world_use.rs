//! Authoritative MoveToObject control for an accepted world-object Use chain.
use bace_gameplay_api::visibility::{AcceptedMoveParameters, ServerMovementGoal};
use bace_geometry::Vec3;
use bace_motion::{MotionIntent, TurnControl, TurnIntent, heading_delta};
use bace_types::EntityId;
use bace_world::World;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorldUseError {
    InvalidRadius,
    MissingActor,
    MissingContent,
}

/// ACE Landblock.WithinUseRadius / WorldObject.IsWithinUseRadiusOf use
/// adjacent-cell cylinder distance, with no combat line-of-sight gate.
pub(crate) fn within_use_radius(
    world: &World,
    actor: EntityId,
    target: EntityId,
    radius: f32,
) -> Result<bool, WorldUseError> {
    if !radius.is_finite() {
        return Err(WorldUseError::InvalidRadius);
    }
    let (actor_cell, _) = world
        .actor_state(actor)
        .map_err(|_| WorldUseError::MissingActor)?;
    let (target_cell, _) = world
        .actor_state(target)
        .map_err(|_| WorldUseError::MissingActor)?;
    let cylinder = |id| -> Result<bace_emotes::SignalCylinder, WorldUseError> {
        let body = world.body(id).map_err(|_| WorldUseError::MissingActor)?;
        let (radius, height) = if let Some(shape) = body.collision_shape() {
            (
                shape
                    .nominal_radius()
                    .ok_or(WorldUseError::MissingContent)?,
                shape
                    .nominal_height()
                    .ok_or(WorldUseError::MissingContent)?,
            )
        } else {
            (body.collision_radius(), body.collision_radius() * 2.0)
        };
        Ok(bace_emotes::SignalCylinder {
            position: body.accepted().position(),
            radius,
            height,
        })
    };
    let from = cylinder(actor)?;
    let mut to = cylinder(target)?;
    if actor_cell != target_cell {
        to.position = to.position
            + world
                .geometry()
                .ok_or(WorldUseError::MissingContent)?
                .frame_offset(actor_cell.0, target_cell.0)
                .map_err(|_| WorldUseError::MissingContent)?;
    }
    let distance =
        bace_emotes::cylinder_distance(from, to).map_err(|_| WorldUseError::MissingContent)? as f32;
    Ok(distance <= radius)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorldApproach {
    pub(crate) control: Option<TurnControl>,
    pub(crate) started_tick: u64,
}

impl WorldApproach {
    pub(crate) fn new(tick: u64) -> Self {
        Self {
            control: None,
            started_tick: tick,
        }
    }

    pub(crate) fn stop(self, world: &mut World, actor: EntityId) {
        if let Some(control) = self.control {
            world.finish_server_move(actor, control);
            if let Ok(body) = world.body_mut(actor) {
                body.finish_server_turn(control);
            }
            world.clear_movement_goal(actor, control);
        }
    }

    pub(crate) fn advance(
        &mut self,
        world: &mut World,
        actor: EntityId,
        target: EntityId,
        use_radius: f32,
    ) -> Result<(), ()> {
        if !use_radius.is_finite() || use_radius < 0.0 {
            return Err(());
        }
        let (cell, accepted) = world.actor_state(actor).map_err(|_| ())?;
        let (target_cell, target_pose) = world.actor_state(target).map_err(|_| ())?;
        let mut target_position = target_pose.position();
        if cell != target_cell {
            target_position = target_position
                + world
                    .geometry()
                    .ok_or(())?
                    .frame_offset(cell.0, target_cell.0)
                    .map_err(|_| ())?;
        }
        let offset = target_position - accepted.position();
        let heading = (-offset.x).atan2(offset.y);
        let delta = heading_delta(accepted.heading_radians(), heading).map_err(|_| ())?;
        let turn = TurnIntent::new((delta / 0.25).clamp(-1.0, 1.0)).map_err(|_| ())?;
        let direction = if delta.abs() < 0.5 {
            offset * (1.0 / offset.length_squared().sqrt().max(0.001))
        } else {
            Vec3::ZERO
        };
        let intent = MotionIntent::new(direction, false).map_err(|_| ())?;
        let control = match self.control {
            Some(control) => control,
            None => world.next_server_control(actor).map_err(|_| ())?,
        };
        if self.control.is_some() {
            world
                .continue_server_move(actor, accepted.epoch(), control, intent)
                .map_err(|_| ())?;
            world
                .body_mut(actor)
                .map_err(|_| ())?
                .continue_server_turn(accepted.epoch(), control, turn)
                .map_err(|_| ())?;
        } else {
            world
                .begin_server_move(actor, accepted.epoch(), control, intent)
                .map_err(|_| ())?;
            if world
                .body_mut(actor)
                .map_err(|_| ())?
                .begin_server_turn(accepted.epoch(), control, turn)
                .is_err()
            {
                world.finish_server_move(actor, control);
                return Err(());
            }
            self.control = Some(control);
        }
        let run_rate = world
            .body(actor)
            .ok()
            .and_then(|body| body.locomotion_run_rate())
            .filter(|rate| (0.0..=20.0).contains(rate))
            .unwrap_or(1.0);
        world
            .bind_movement_goal(
                actor,
                control,
                ServerMovementGoal::MoveToObject {
                    target,
                    parameters: AcceptedMoveParameters {
                        flags: 0x1f09eff0,
                        distance_to_object: use_radius,
                        min_distance: 0.1,
                        fail_distance: 15.0,
                        speed: 1.0,
                        walk_run_threshold: 15.0,
                        desired_heading: 0.0,
                    },
                    run_rate,
                },
            )
            .map_err(|_| ())
    }
}
