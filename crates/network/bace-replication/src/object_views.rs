//! Accepted World evidence to pinned ACE object/motion/position layouts. One
//! object projection is shared by all recipients; it owns no physical state.
use crate::{ReplicationMessage, SequenceKind as K, Sequences, SessionProjectionError as Error};
use bace_gameplay_api::visibility::*;
use bace_wire::*;

#[derive(Clone, Debug)]
pub struct ObjectProjection {
    pub view: AcceptedObjectView,
    pub description: ObjectDescription,
    pub create: Vec<u8>,
    pub updates: Vec<ReplicationMessage>,
    action_sequences: Vec<(AcceptedMotionAction, u16)>,
}
impl ObjectProjection {
    pub fn prepare(
        source: &ObjectDescription,
        view: AcceptedObjectView,
        previous: Option<&Self>,
        sequences: &mut Sequences,
        limits: ObjectCodecLimits,
    ) -> Result<Self, Error> {
        Self::prepare_with_force(source, view, previous, sequences, limits, false)
    }
    pub fn prepare_with_force(
        source: &ObjectDescription,
        view: AcceptedObjectView,
        previous: Option<&Self>,
        sequences: &mut Sequences,
        limits: ObjectCodecLimits,
        force: bool,
    ) -> Result<Self, Error> {
        Self::prepare_with_public_state(source, view, previous, sequences, limits, force, None)
    }
    /// Already projected public flags come from the canonical output owner, never
    /// from a client pose or the immutable initial-login blueprint.
    pub fn prepare_with_public_state(
        source: &ObjectDescription,
        view: AcceptedObjectView,
        previous: Option<&Self>,
        sequences: &mut Sequences,
        limits: ObjectCodecLimits,
        force: bool,
        public_state: Option<u32>,
    ) -> Result<Self, Error> {
        if source.object_id != view.entity.0
            || view.entity.0 == 0
            || view.cell == 0
            || !view.heading_radians.is_finite()
            || !view
                .position
                .into_iter()
                .chain(view.velocity)
                .all(f32::is_finite)
        {
            return Err(Error::InvalidProjection);
        }
        let motion = view.motion.as_ref().map_err(|_| Error::InvalidProjection)?;
        if motion.as_ref().is_some_and(|m| m.actions.len() > 64) {
            return Err(Error::Limit);
        }
        let mut counters = sequences.object_preview();
        let position_changed = force
            || previous.is_some_and(|old| {
                old.view.cell != view.cell
                    || old.view.position != view.position
                    || old.view.velocity != view.velocity
                    || old.view.heading_radians != view.heading_radians
                    || old.view.grounded != view.grounded
                    || old.view.epoch != view.epoch
            });
        let mut motion_changed = previous.is_some_and(|old| old.view.motion != view.motion);
        let state_changed = previous.is_some_and(|old| old.view.held != view.held);
        if let Some(old) = previous
            && view.epoch != old.view.epoch
            && view.epoch.wrapping_sub(old.view.epoch) >= 0x8000
        {
            return Err(Error::InvalidProjection);
        }
        // Body owns accepted teleport epochs; never manufacture one from a packet.
        counters
            .accepted_teleport(view.epoch)
            .map_err(|_| Error::Limit)?;
        for (change, kind) in [
            (position_changed, K::ObjectPosition),
            (motion_changed, K::ObjectMovement),
            (
                motion_changed && motion.as_ref().is_none_or(|m| !m.autonomous),
                K::ObjectServerControl,
            ),
            (force, K::ObjectForcePosition),
            (state_changed, K::ObjectState),
        ] {
            if change {
                counters.advance(kind, 0).map_err(|_| Error::Limit)?;
            }
        }
        let mut action_sequences = Vec::new();
        if let Some(motion) = motion {
            for action in &motion.actions {
                let prior = previous.and_then(|p| {
                    p.action_sequences.iter().find(|(a, _)| {
                        a.domain == action.domain
                            && a.owner == action.owner
                            && a.sequence == action.sequence
                    })
                });
                let sequence = match prior {
                    Some((_, sequence)) => *sequence,
                    None => counters.advance(K::Motion, 0).map_err(|_| Error::Limit)?,
                };
                action_sequences.push((*action, sequence));
            }
        }
        let movement = motion
            .as_ref()
            .map(|motion| project_motion(motion, &action_sequences))
            .transpose()?;
        if previous.is_none()
            && let Some(PhysicsMovement::Motion(old)) = &source.physics.options.movement
            && movement.as_ref() != Some(old)
        {
            motion_changed = true;
            counters
                .advance(K::ObjectMovement, 0)
                .map_err(|_| Error::Limit)?;
            if movement.as_ref().is_none_or(|m| !m.autonomous) {
                counters
                    .advance(K::ObjectServerControl, 0)
                    .map_err(|_| Error::Limit)?;
            }
        }
        let half = view.heading_radians * 0.5;
        let position = WirePosition {
            cell: view.cell,
            origin: view.position,
            rotation: [half.cos(), 0.0, 0.0, half.sin()],
        };
        let mut description = source.clone();
        if let Some(state) = public_state {
            description.physics.state = state;
        }
        description.physics.options.position = Some(position);
        description.physics.options.velocity = Some(view.velocity);
        if let Some(movement) = &movement {
            description.physics.options.movement = Some(PhysicsMovement::Motion(movement.clone()));
        } else if matches!(
            description.physics.options.movement,
            Some(PhysicsMovement::Motion(_))
        ) {
            return Err(Error::InvalidProjection);
        }
        if view.held {
            description.physics.state |= 0x01000000;
        }
        let seq = physics_sequences(&counters);
        description.physics.sequences = seq;
        let create = description.encode_create(limits)?;
        let mut updates = Vec::new();
        if state_changed {
            updates.push(message(
                ObjectControl::SetState {
                    object_id: source.object_id,
                    state: description.physics.state,
                    instance_sequence: seq.instance,
                    state_sequence: seq.state,
                }
                .encode(),
            ));
        }
        if position_changed {
            updates.push(message(
                PositionUpdate {
                    object_id: source.object_id,
                    pack: PositionPack {
                        position,
                        velocity: Some(view.velocity),
                        placement: match source.physics.options.movement {
                            Some(PhysicsMovement::AnimationFrame(frame)) => Some(frame),
                            _ => None,
                        },
                        grounded: view.grounded,
                        instance_sequence: seq.instance,
                        position_sequence: seq.position,
                        teleport_sequence: seq.teleport,
                        force_position_sequence: seq.force_position,
                    },
                }
                .encode(),
            ));
        }
        if motion_changed && let Some(movement) = movement {
            updates.push(message(
                MotionUpdate {
                    object_id: source.object_id,
                    instance_sequence: seq.instance,
                    movement_sequence: seq.movement,
                    server_control_sequence: seq.server_control,
                    autonomous: movement.autonomous,
                    motion_flags: movement.motion_flags,
                    current_style: movement.current_style,
                    body: movement.body,
                }
                .encode(limits.max_motion_commands)?,
            ));
        }
        if updates
            .iter()
            .any(|m| m.bytes.len() > limits.max_message_bytes)
        {
            return Err(Error::Limit);
        }
        sequences
            .commit_object_preview(counters)
            .map_err(|_| Error::Limit)?;
        Ok(Self {
            view,
            description,
            create,
            updates,
            action_sequences,
        })
    }
}
fn message(bytes: Vec<u8>) -> ReplicationMessage {
    ReplicationMessage { queue: 10, bytes }
}
pub fn physics_sequences(s: &Sequences) -> PhysicsSequences {
    PhysicsSequences {
        position: s.current(K::ObjectPosition, 0),
        movement: s.current(K::ObjectMovement, 0),
        state: s.current(K::ObjectState, 0),
        vector: s.current(K::ObjectVector, 0),
        teleport: s.current(K::ObjectTeleport, 0),
        server_control: s.current(K::ObjectServerControl, 0),
        force_position: s.current(K::ObjectForcePosition, 0),
        visual_description: s.current(K::ObjectVisualDesc, 0),
        instance: s.current(K::ObjectInstance, 0),
    }
}
fn project_motion(
    m: &AcceptedObjectMotion,
    actions: &[(AcceptedMotionAction, u16)],
) -> Result<MovementDescription, Error> {
    if ![m.forward_rate, m.sidestep_rate, m.turn_rate]
        .into_iter()
        .all(f32::is_finite)
    {
        return Err(Error::InvalidProjection);
    }
    let body = if let Some(goal) = m.goal {
        match goal.goal {
            ServerMovementGoal::MoveToObject {
                target,
                parameters,
                run_rate,
            } => {
                let (cell, origin) = goal.target_position.ok_or(Error::InvalidProjection)?;
                MotionBody::MoveToObject {
                    target: target.0,
                    cell,
                    origin,
                    parameters: move_parameters(parameters),
                    run_rate,
                }
            }
            ServerMovementGoal::MoveToPosition {
                cell,
                position,
                parameters,
                run_rate,
            } => MotionBody::MoveToPosition {
                cell,
                origin: position,
                parameters: move_parameters(parameters),
                run_rate,
            },
            ServerMovementGoal::TurnToObject { target, parameters } => MotionBody::TurnToObject {
                target: target.0,
                desired_heading: parameters.desired_heading,
                parameters: turn_parameters(parameters),
            },
            ServerMovementGoal::TurnToHeading { parameters } => MotionBody::TurnToHeading {
                parameters: turn_parameters(parameters),
            },
        }
    } else {
        MotionBody::State {
            state: InterpretedMotion {
                current_style: Some(m.style as u16),
                forward_command: Some(m.forward_motion as u16),
                sidestep_command: (m.sidestep_rate != 0.0).then_some(0x0f),
                turn_command: (m.turn_rate != 0.0).then_some(0x0d),
                forward_speed: Some(m.forward_rate),
                sidestep_speed: (m.sidestep_rate != 0.0).then_some(m.sidestep_rate),
                turn_speed: (m.turn_rate != 0.0).then_some(m.turn_rate),
                commands: actions
                    .iter()
                    .map(|(a, sequence)| MotionCommandItem {
                        raw_command: a.motion as u16,
                        sequence: *sequence,
                        autonomous: false,
                        speed: a.speed,
                    })
                    .collect(),
            },
            sticky_object: None,
        }
    };
    Ok(MovementDescription {
        autonomous: m.autonomous,
        motion_flags: 0,
        current_style: m.style as u16,
        body,
    })
}
/// Project one frozen simulation motion into the pinned ACE movement layout.
/// The caller's canonical sequence owner assigns motion command numbers when
/// `project_server_motion` encodes this view for private and observer delivery.
pub fn project_accepted_server_motion(
    view: &AcceptedObjectView,
) -> Result<MovementDescription, Error> {
    let motion = view
        .motion
        .as_ref()
        .map_err(|_| Error::InvalidProjection)?
        .as_ref()
        .ok_or(Error::InvalidProjection)?;
    if motion.autonomous || motion.actions.len() > 32 {
        return Err(Error::InvalidProjection);
    }
    let actions = motion
        .actions
        .iter()
        .copied()
        .map(|action| (action, 0))
        .collect::<Vec<_>>();
    project_motion(motion, &actions)
}
fn move_parameters(p: AcceptedMoveParameters) -> MoveToParameters {
    MoveToParameters {
        flags: p.flags,
        distance_to_object: p.distance_to_object,
        min_distance: p.min_distance,
        fail_distance: p.fail_distance,
        speed: p.speed,
        walk_run_threshold: p.walk_run_threshold,
        desired_heading: p.desired_heading,
    }
}
fn turn_parameters(p: AcceptedTurnParameters) -> TurnToParameters {
    TurnToParameters {
        flags: p.flags,
        speed: p.speed,
        desired_heading: p.desired_heading,
    }
}
impl ObjectProjection {
    /// Current complete motion/position state for a recipient that may have
    /// skipped older snapshots. Uses the already-owned counters unchanged.
    pub fn refresh_messages(
        &self,
        limits: ObjectCodecLimits,
    ) -> Result<Vec<ReplicationMessage>, Error> {
        let p = &self.description.physics;
        let s = p.sequences;
        let position = p.options.position.ok_or(Error::InvalidProjection)?;
        let mut out = vec![
            message(
                ObjectControl::SetState {
                    object_id: self.view.entity.0,
                    state: p.state,
                    instance_sequence: s.instance,
                    state_sequence: s.state,
                }
                .encode(),
            ),
            message(
                PositionUpdate {
                    object_id: self.view.entity.0,
                    pack: PositionPack {
                        position,
                        velocity: p.options.velocity,
                        placement: match p.options.movement {
                            Some(PhysicsMovement::AnimationFrame(frame)) => Some(frame),
                            _ => None,
                        },
                        grounded: self.view.grounded,
                        instance_sequence: s.instance,
                        position_sequence: s.position,
                        teleport_sequence: s.teleport,
                        force_position_sequence: s.force_position,
                    },
                }
                .encode(),
            ),
        ];
        if let Some(PhysicsMovement::Motion(m)) = &p.options.movement {
            out.push(message(
                MotionUpdate {
                    object_id: self.view.entity.0,
                    instance_sequence: s.instance,
                    movement_sequence: s.movement,
                    server_control_sequence: s.server_control,
                    autonomous: m.autonomous,
                    motion_flags: m.motion_flags,
                    current_style: m.current_style,
                    body: m.body.clone(),
                }
                .encode(limits.max_motion_commands)?,
            ));
        }
        Ok(out)
    }
}
