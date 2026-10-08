//! ACE GameMessageUpdateMotion / MovementDataExtensions.Write / MotionItem.Write
//! at 47edade3bd3f6044b676d4eb877c4965c7eda62b. One object counter owner produces
//! identical bytes for all observers. This projection never completes an action.
use crate::{
    BatchLimits, ReplicationMessage, SequenceKind as Kind, Sequences,
    SessionProjectionError as Error,
};
use bace_wire::{MotionBody, MotionUpdate, MovementDescription};
const MAX_COMMANDS: usize = 32;
pub fn project_server_motion(
    object: u32,
    view: &MovementDescription,
    sequences: &mut Sequences,
    limits: BatchLimits,
) -> Result<ReplicationMessage, Error> {
    if object == 0 || view.autonomous {
        return Err(Error::InvalidProjection);
    }
    let count = match &view.body {
        MotionBody::State { state, .. } => state.commands.len(),
        _ => 0,
    };
    if count > MAX_COMMANDS || limits.max_messages == 0 {
        return Err(Error::Limit);
    }
    let keys = [
        (Kind::ObjectMovement, 0),
        (Kind::ObjectServerControl, 0),
        (Kind::Motion, 0),
    ];
    sequences
        .check_capacity(&keys[..if count == 0 { 2 } else { 3 }])
        .map_err(|_| Error::Limit)?;
    validate(&view.body)?;
    let mut body = view.body.clone();
    if let MotionBody::State { state, .. } = &mut body {
        let mut sequence = sequences.current(Kind::Motion, 0);
        for command in &mut state.commands {
            sequence = if sequence == 0x7fff { 0 } else { sequence + 1 };
            command.sequence = sequence;
            command.autonomous = false;
        }
    }
    let bytes = MotionUpdate {
        object_id: object,
        instance_sequence: sequences.current(Kind::ObjectInstance, 0),
        movement_sequence: sequences.current(Kind::ObjectMovement, 0).wrapping_add(1),
        server_control_sequence: sequences
            .current(Kind::ObjectServerControl, 0)
            .wrapping_add(1),
        autonomous: false,
        motion_flags: view.motion_flags,
        current_style: view.current_style,
        body,
    }
    .encode(MAX_COMMANDS)?;
    if bytes.len() > limits.max_message_bytes || bytes.len() > limits.max_bytes {
        return Err(Error::Limit);
    }
    // Capacity and encoding were preflighted together. No failed prefix can
    // advance counters; reliable output retains these exact bytes on retry.
    if count == 0 {
        sequences
            .advance_batch([keys[0], keys[1]])
            .expect("checked capacity");
    } else {
        sequences.advance_batch(keys).expect("checked capacity");
        for _ in 1..count {
            sequences
                .advance(Kind::Motion, 0)
                .expect("existing motion counter");
        }
    }
    Ok(ReplicationMessage { queue: 10, bytes })
}
fn validate(body: &MotionBody) -> Result<(), Error> {
    let valid = match body {
        MotionBody::State { state, .. } => {
            state
                .forward_speed
                .into_iter()
                .chain(state.sidestep_speed)
                .chain(state.turn_speed)
                .all(f32::is_finite)
                && state.commands.iter().all(|c| c.speed.is_finite())
        }
        MotionBody::MoveToObject {
            target,
            cell,
            origin,
            parameters,
            run_rate,
        } => {
            *target != 0
                && *cell != 0
                && origin.iter().all(|v| v.is_finite())
                && run_rate.is_finite()
                && move_parameters(*parameters)
        }
        MotionBody::MoveToPosition {
            cell,
            origin,
            parameters,
            run_rate,
        } => {
            *cell != 0
                && origin.iter().all(|v| v.is_finite())
                && run_rate.is_finite()
                && move_parameters(*parameters)
        }
        MotionBody::TurnToObject {
            target,
            desired_heading,
            parameters,
        } => {
            *target != 0
                && desired_heading.is_finite()
                && parameters.speed.is_finite()
                && parameters.desired_heading.is_finite()
        }
        MotionBody::TurnToHeading { parameters } => {
            parameters.speed.is_finite() && parameters.desired_heading.is_finite()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidProjection)
    }
}
fn move_parameters(p: bace_wire::MoveToParameters) -> bool {
    [
        p.distance_to_object,
        p.min_distance,
        p.fail_distance,
        p.speed,
        p.walk_run_threshold,
        p.desired_heading,
    ]
    .iter()
    .all(|v| v.is_finite())
}
