//! Script movement requests bounded server intent; only swept accepted physics
//! can satisfy the destination. Cross-cell asset/portal admission stays explicit.
use super::Kernel;
use crate::{NpcEffect, NpcProposal, npc::NpcMoveState};
use bace_gameplay_api::visibility::{
    AcceptedMoveParameters, AcceptedTurnParameters, ServerMovementGoal,
};
use bace_gameplay_api::{NpcCompletion, NpcDestination, NpcFailure as E, NpcOperation};
use bace_geometry::Vec3;
use bace_motion::{MotionIntent, TurnIntent, heading_delta};

fn heading(rotation: [f32; 4]) -> Result<f32, E> {
    let [w, x, y, z] = rotation;
    let norm = w * w + x * x + y * y + z * z;
    if !norm.is_finite() || norm < 0.000001 {
        return Err(E::InvalidInput);
    }
    Ok((2.0 * (w * z + x * y)).atan2(norm - 2.0 * (y * y + z * z)))
}
fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [w, x, y, z] = a;
    let [v, i, j, k] = b;
    [
        w * v - x * i - y * j - z * k,
        w * i + x * v + y * k - z * j,
        w * j - x * k + y * v + z * i,
        w * k + x * j - y * i + z * v,
    ]
}
fn turn_axis(current: f32, target: f32, rate: f32) -> Result<TurnIntent, E> {
    let delta = heading_delta(current, target).map_err(|_| E::InvalidInput)?;
    if rate <= 0.0 && delta.abs() > 0.001 {
        return Err(E::MissingContent);
    }
    TurnIntent::new(if rate > 0.0 {
        (delta / (rate / 30.0)).clamp(-1.0, 1.0)
    } else {
        0.0
    })
    .map_err(|_| E::InvalidInput)
}
impl Kernel {
    pub fn begin_npc_give(&mut self, expected: &NpcProposal) -> Result<(bool, bool), E> {
        self.npcs.validate_service(expected)?;
        if !matches!(
            expected.effect,
            NpcEffect::Service(NpcOperation::Give { .. })
        ) {
            return Err(E::Unsupported);
        }
        if self.npcs.service_detached(expected.ticket) {
            return Ok((true, false));
        }
        let source = expected.context.source;
        let no_turn = matches!(
            self.world
                .properties(source)
                .and_then(|p| p.get(bace_entity::PropertyFamily::Bool, 82)),
            Some(bace_entity::PropertyValue::Bool(true))
        );
        if no_turn || self.world.combatant(source).is_none() {
            return Ok((false, false));
        }
        self.begin_npc_movement(expected, None)?;
        Ok((false, true))
    }

    pub fn apply_npc_reset_home(&mut self, expected: &NpcProposal) -> Result<NpcDestination, E> {
        self.npcs.validate_service(expected)?;
        if !matches!(expected.effect, NpcEffect::Service(NpcOperation::ResetHome)) {
            return Err(E::Unsupported);
        }
        let home = self
            .population
            .reset_script_home(expected.context.source, &self.world)
            .map_err(|_| E::MissingActor)?;
        self.npcs
            .mark_service_adopted(expected, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.confirm_npc_committed(expected)?;
        Ok(home)
    }
    /// `home` is the immutable owner-supplied home position used by Move/MoveHome.
    /// It must be restored from content/accepted ResetHome state, never a client.
    pub fn begin_npc_movement(
        &mut self,
        expected: &NpcProposal,
        home: Option<NpcDestination>,
    ) -> Result<(), E> {
        self.npcs.validate_service(expected)?;
        if self.npcs.moves.contains_key(&expected.ticket) {
            return Err(E::Conflict);
        }
        let actor = expected.context.source;
        let home = home.or_else(|| self.population.script_home(actor));
        if self.npcs.reserved_except(actor, expected.ticket) {
            return Err(E::DurabilityPending);
        }
        let cell = self
            .world
            .actor_state(actor)
            .map_err(|_| E::MissingActor)?
            .0;
        let body = self.world.body(actor).map_err(|_| E::MissingActor)?;
        let accepted = body.accepted();
        let (mut destination, turn_only, speed) = match expected.effect {
            NpcEffect::Service(NpcOperation::Move {
                home: go_home,
                absolute,
                destination,
                extent,
            }) => {
                let destination = if go_home {
                    home.ok_or(E::MissingContent)?
                } else if absolute {
                    destination
                } else {
                    let home = home.ok_or(E::MissingContent)?;
                    NpcDestination {
                        cell: home.cell,
                        position: home.position + destination.position,
                        rotation: multiply(home.rotation, destination.rotation),
                        relative: false,
                    }
                };
                (destination, false, extent)
            }
            NpcEffect::Service(NpcOperation::Give { .. }) => {
                let target = expected.context.target.ok_or(E::MissingActor)?;
                let (target_cell, target_state) = self
                    .world
                    .actor_state(target)
                    .map_err(|_| E::MissingActor)?;
                if target_cell != cell {
                    return Err(E::MissingContent);
                }
                let delta = target_state.position() - accepted.position();
                let yaw = (-delta.x).atan2(delta.y);
                (
                    NpcDestination {
                        cell: Some(cell),
                        position: accepted.position(),
                        rotation: [(yaw * 0.5).cos(), 0.0, 0.0, (yaw * 0.5).sin()],
                        relative: false,
                    },
                    true,
                    1.0,
                )
            }
            NpcEffect::Service(NpcOperation::Turn { target, rotation }) => {
                let rotation = if target {
                    let target = expected.context.target.ok_or(E::MissingActor)?;
                    if self.world.actor_state(target).ok().map(|s| s.0) != Some(cell) {
                        return Err(E::MissingContent);
                    }
                    let delta = self
                        .world
                        .body(target)
                        .map_err(|_| E::MissingActor)?
                        .accepted()
                        .position()
                        - accepted.position();
                    let yaw = (-delta.x).atan2(delta.y);
                    [(yaw * 0.5).cos(), 0.0, 0.0, (yaw * 0.5).sin()]
                } else {
                    rotation
                };
                (
                    NpcDestination {
                        cell: Some(cell),
                        position: accepted.position(),
                        rotation,
                        relative: false,
                    },
                    true,
                    1.0,
                )
            }
            _ => return Err(E::Unsupported),
        };
        destination.cell = Some(destination.cell.unwrap_or(cell));
        if destination.cell != Some(cell) || !destination.position.is_finite() {
            return Err(E::MissingContent);
        }
        let target_heading = heading(destination.rotation)?;
        let delta = destination.position - accepted.position();
        let approach_heading = if !turn_only && delta.length_squared() > 0.36 {
            (-delta.x).atan2(delta.y)
        } else {
            target_heading
        };
        let turn = turn_axis(
            accepted.heading_radians(),
            approach_heading,
            body.maximum_turn_rate(),
        )?;
        let desired_heading = target_heading.to_degrees().rem_euclid(360.0);
        let goal = if turn_only {
            let parameters = AcceptedTurnParameters {
                flags: 0x1EE0F,
                speed: 1.0,
                desired_heading,
            };
            if matches!(
                expected.effect,
                NpcEffect::Service(
                    NpcOperation::Turn { target: true, .. } | NpcOperation::Give { .. }
                )
            ) {
                ServerMovementGoal::TurnToObject {
                    target: expected.context.target.ok_or(E::MissingActor)?,
                    parameters: AcceptedTurnParameters {
                        desired_heading: 0.0,
                        ..parameters
                    },
                }
            } else {
                ServerMovementGoal::TurnToHeading { parameters }
            }
        } else {
            let run_rate = body
                .locomotion_run_rate()
                .or_else(|| body.collision_shape().is_none().then_some(1.0))
                .ok_or(E::MissingContent)?;
            ServerMovementGoal::MoveToPosition {
                cell: cell.0,
                position: [
                    destination.position.x,
                    destination.position.y,
                    destination.position.z,
                ],
                run_rate,
                parameters: AcceptedMoveParameters {
                    flags: 0x1EE4F,
                    distance_to_object: 0.6,
                    min_distance: 0.0,
                    fail_distance: f32::MAX,
                    speed,
                    walk_run_threshold: 15.0,
                    desired_heading,
                },
            }
        };
        let control = self
            .world
            .next_server_control(actor)
            .map_err(|_| E::Conflict)?;
        let deadline = self.tick.checked_add(30 * 120).ok_or(E::Capacity)?;
        let intent = MotionIntent::new(
            if turn_only {
                Vec3::ZERO
            } else {
                destination.position - accepted.position()
            },
            false,
        )
        .map_err(|_| E::InvalidInput)?;
        self.world
            .begin_server_move_scaled(actor, accepted.epoch(), control, intent, speed)
            .map_err(|_| E::Conflict)?;
        if self
            .world
            .body_mut(actor)
            .map_err(|_| E::MissingActor)?
            .begin_server_turn(accepted.epoch(), control, turn)
            .is_err()
        {
            self.world.finish_server_move(actor, control);
            return Err(E::Conflict);
        }
        if self.world.bind_movement_goal(actor, control, goal).is_err() {
            self.world.finish_server_move(actor, control);
            self.world
                .body_mut(actor)
                .map_err(|_| E::MissingActor)?
                .finish_server_turn(control);
            return Err(E::MissingContent);
        }
        self.npcs.moves.insert(
            expected.ticket,
            NpcMoveState {
                proposal: expected.clone(),
                actor,
                epoch: accepted.epoch(),
                control,
                destination,
                turn_only,
                speed,
                deadline,
            },
        );
        Ok(())
    }
    /// Poll on the simulation owner after physics. No elapsed-time guess can
    /// turn a blocked sweep into an arrival. A cancelled/expired move retains
    /// the authored row and must be explicitly retried or failed by its owner.
    pub fn poll_npc_movement(&mut self, expected: &NpcProposal) -> Result<bool, E> {
        let state = self.npcs.moves.get(&expected.ticket).ok_or(E::Conflict)?;
        if state.proposal != *expected {
            return Err(E::Conflict);
        }
        let body = self.world.body(state.actor).map_err(|_| E::MissingActor)?;
        let accepted = body.accepted();
        if self.tick >= state.deadline
            || accepted.epoch() != state.epoch
            || body.server_move() != Some(state.control)
            || body.server_turn() != Some(state.control)
            || self.world.actor_state(state.actor).ok().map(|s| s.0) != state.destination.cell
        {
            return Err(E::Cancelled);
        }
        let delta = state.destination.position - accepted.position();
        let target = if matches!(
            expected.effect,
            NpcEffect::Service(NpcOperation::Turn { target: true, .. } | NpcOperation::Give { .. })
        ) {
            let target = expected.context.target.ok_or(E::MissingActor)?;
            let (cell, position) = self
                .world
                .actor_state(target)
                .map_err(|_| E::MissingActor)?;
            if Some(cell) != state.destination.cell {
                return Err(E::MissingContent);
            }
            let delta = position.position() - accepted.position();
            (-delta.x).atan2(delta.y)
        } else {
            heading(state.destination.rotation)?
        };
        let arrived = state.turn_only || delta.length_squared() <= 0.36;
        let target = if !arrived {
            (-delta.x).atan2(delta.y)
        } else {
            target
        };
        let facing = heading_delta(accepted.heading_radians(), target)
            .map_err(|_| E::InvalidInput)?
            .abs()
            <= 0.001;
        if arrived && facing {
            let (actor, control) = (state.actor, state.control);
            self.world.clear_movement_goal(actor, control);
            self.world.finish_server_move(actor, control);
            self.world
                .body_mut(actor)
                .map_err(|_| E::MissingActor)?
                .finish_server_turn(control);
            if matches!(
                expected.effect,
                NpcEffect::Service(NpcOperation::Give { .. })
            ) {
                self.npcs.moves.remove(&expected.ticket);
                return Ok(true);
            }
            self.npcs
                .mark_service_adopted(expected, NpcCompletion::Applied { post_delay: 0.0 })?;
            self.npcs.moves.remove(&expected.ticket);
            self.confirm_npc_committed(expected)?;
            return Ok(true);
        }
        let (actor, epoch, control) = (state.actor, state.epoch, state.control);
        let turn = turn_axis(accepted.heading_radians(), target, body.maximum_turn_rate())?;
        self.world
            .continue_server_move_scaled(
                actor,
                epoch,
                control,
                MotionIntent::new(if arrived { Vec3::ZERO } else { delta }, false)
                    .map_err(|_| E::InvalidInput)?,
                state.speed,
            )
            .map_err(|_| E::Conflict)?;
        self.world
            .body_mut(actor)
            .map_err(|_| E::MissingActor)?
            .continue_server_turn(epoch, control, turn)
            .map_err(|_| E::Conflict)?;
        Ok(false)
    }
    pub fn cancel_npc_movement(&mut self, expected: &NpcProposal) -> Result<(), E> {
        let state = self.npcs.moves.get(&expected.ticket).ok_or(E::Conflict)?;
        if state.proposal != *expected {
            return Err(E::Conflict);
        }
        self.world.clear_movement_goal(state.actor, state.control);
        self.world.finish_server_move(state.actor, state.control);
        self.world
            .body_mut(state.actor)
            .map_err(|_| E::MissingActor)?
            .finish_server_turn(state.control);
        self.npcs.moves.remove(&expected.ticket);
        Ok(())
    }
}
