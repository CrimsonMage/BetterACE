//! Read-only renderer evidence from the single accepted body and action owner.
use super::*;
use bace_gameplay_api::visibility::*;
use bace_motion::{MotionDomain, TurnControl};
impl World {
    pub fn bind_movement_goal(
        &mut self,
        actor: EntityId,
        control: TurnControl,
        goal: ServerMovementGoal,
    ) -> Result<(), WorldError> {
        let body = self.body(actor)?;
        if body.server_move() != Some(control) && body.server_turn() != Some(control) {
            return Err(WorldError::InvalidMotion);
        }
        let valid = match goal {
            ServerMovementGoal::MoveToObject {
                target,
                parameters,
                run_rate,
            } => {
                target != actor
                    && self.actors.contains_key(&target)
                    && valid_move(parameters, run_rate)
            }
            ServerMovementGoal::MoveToPosition {
                cell,
                position,
                parameters,
                run_rate,
            } => {
                cell != 0
                    && position.into_iter().all(f32::is_finite)
                    && valid_move(parameters, run_rate)
            }
            ServerMovementGoal::TurnToObject { target, parameters } => {
                target != actor && self.actors.contains_key(&target) && valid_turn(parameters)
            }
            ServerMovementGoal::TurnToHeading { parameters } => valid_turn(parameters),
        };
        if !valid || !self.movement_goals.contains_key(&actor) && self.movement_goals.len() >= 65536
        {
            return Err(WorldError::InvalidMotion);
        }
        self.movement_goals.insert(actor, (control, goal));
        Ok(())
    }
    pub fn clear_movement_goal(&mut self, actor: EntityId, control: TurnControl) {
        if self
            .movement_goals
            .get(&actor)
            .is_some_and(|(old, _)| *old == control)
        {
            self.movement_goals.remove(&actor);
        }
    }
    pub fn accepted_object_view(
        &self,
        entity: EntityId,
    ) -> Result<AcceptedObjectView, ObjectViewRejection> {
        if let Some(actor) = self.actors.get(&entity) {
            let state = actor.body.accepted();
            let p = state.position();
            let v = state.velocity();
            return Ok(AcceptedObjectView {
                entity,
                cell: actor.cell.0,
                position: [p.x, p.y, p.z],
                velocity: [v.x, v.y, v.z],
                heading_radians: state.heading_radians(),
                grounded: state.grounded(),
                epoch: state.epoch(),
                held: self.retirement_held(entity),
                motion: self.accepted_object_motion(entity),
            });
        }
        if let Some(projectile) = self.projectiles.get(&entity) {
            let p = projectile.body.position();
            let v = projectile.body.velocity();
            return Ok(AcceptedObjectView {
                entity,
                cell: projectile.cell.0,
                position: [p.x, p.y, p.z],
                velocity: [v.x, v.y, v.z],
                heading_radians: (-v.x).atan2(v.y),
                grounded: false,
                epoch: 0,
                held: projectile.body.finished(),
                motion: Ok(None),
            });
        }
        Err(ObjectViewRejection::MissingActor)
    }
    fn accepted_object_motion(
        &self,
        entity: EntityId,
    ) -> Result<Option<AcceptedObjectMotion>, ObjectViewRejection> {
        let body = &self
            .actors
            .get(&entity)
            .ok_or(ObjectViewRejection::MissingActor)?
            .body;
        let owned = self
            .motions
            .get(&entity)
            .filter(|m| m.epoch == body.accepted().epoch());
        let source = self.source_motion_state(entity);
        let drive = body.locomotion_projection();
        let ending_ready = owned.is_some_and(|m| {
            (m.ending || m.playback.requested_chain().motion == 0x41000003)
                && m.playback.cursor().completed
                && m.playback.chain().cyclic_is_rootless()
        }) && source.is_some_and(|s| s.substate == 0x41000003);
        if ending_ready
            && let (Some(s), Some((style, _))) = (source, drive)
            && s.style != style
        {
            return Err(ObjectViewRejection::MissingMotion);
        }
        let goal = if let Some((control, goal)) = self
            .movement_goals
            .get(&entity)
            .filter(|(c, _)| body.server_move() == Some(*c) || body.server_turn() == Some(*c))
        {
            let target_position = match goal {
                ServerMovementGoal::MoveToObject { target, .. } => {
                    let (cell, state) = self
                        .actor_state(*target)
                        .map_err(|_| ObjectViewRejection::MissingActor)?;
                    let p = state.position();
                    Some((cell.0, [p.x, p.y, p.z]))
                }
                _ => None,
            };
            Some(AcceptedMovementGoal {
                control_owner: control.owner,
                control_sequence: control.sequence,
                goal: *goal,
                target_position,
            })
        } else {
            None
        };
        if (body.server_move().is_some() || body.server_turn().is_some()) && goal.is_none() {
            return Err(ObjectViewRejection::MissingMotion);
        }
        let mut actions = Vec::new();
        if let Some(owned) = owned {
            for (token, motion, speed) in owned.playback.pending_actions() {
                if motion & 0x10000000 == 0 {
                    continue;
                }
                if actions.len() == 64 {
                    return Err(ObjectViewRejection::Capacity);
                }
                actions.push(AcceptedMotionAction {
                    domain: match token.domain {
                        MotionDomain::Casting => AcceptedMotionDomain::Casting,
                        MotionDomain::Physical => AcceptedMotionDomain::Physical,
                        MotionDomain::Interaction => AcceptedMotionDomain::Interaction,
                        MotionDomain::Inventory => AcceptedMotionDomain::Inventory,
                        MotionDomain::Crafting => AcceptedMotionDomain::Crafting,
                        MotionDomain::Recall => AcceptedMotionDomain::Recall,
                        MotionDomain::Death => AcceptedMotionDomain::Death,
                    },
                    owner: token.owner,
                    sequence: token.sequence,
                    motion,
                    speed,
                });
            }
        }
        let (style, forward_motion, forward_rate, sidestep_rate, turn_rate) =
            if let Some((style, drive)) = drive.filter(|_| source.is_none() || ending_ready) {
                (
                    style,
                    drive.forward_motion,
                    drive.forward_rate,
                    drive.side_rate,
                    drive.turn_rate,
                )
            } else if let Some(source) = source {
                (source.style, source.substate, source.speed, 0.0, 0.0)
            } else {
                if body.has_locomotion_intent() || goal.is_some() {
                    return Err(ObjectViewRejection::MissingMotion);
                }
                return Ok(None);
            };
        Ok(Some(AcceptedObjectMotion {
            autonomous: goal.is_none()
                && (source.is_none() || ending_ready)
                && body.locomotion_autonomous(),
            style,
            forward_motion,
            forward_rate,
            sidestep_rate,
            turn_rate,
            actions,
            goal,
        }))
    }
}
fn valid_move(p: AcceptedMoveParameters, run: f32) -> bool {
    [
        p.distance_to_object,
        p.min_distance,
        p.fail_distance,
        p.speed,
        p.walk_run_threshold,
        p.desired_heading,
        run,
    ]
    .into_iter()
    .all(f32::is_finite)
        && p.distance_to_object >= 0.0
        && p.min_distance >= 0.0
        && p.fail_distance >= 0.0
        && p.speed > 0.0
        && p.speed <= 20.0
        && (0.0..=20.0).contains(&run)
}
fn valid_turn(p: AcceptedTurnParameters) -> bool {
    p.speed.is_finite() && p.speed > 0.0 && p.speed <= 20.0 && p.desired_heading.is_finite()
}
