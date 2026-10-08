//! Source raw-motion defaults; accepted physics never comes from observations.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::{
    ActionContext,
    locomotion::{LocomotionRequest, RawLocomotionState},
};
use bace_wire::{
    ClientAutonomousPosition, ClientJump, ClientMoveToState, GameActionEnvelope, MovementEpochs,
    WireError, WirePosition, opcode::GameActionType,
};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LocomotionObservations {
    State {
        position: WirePosition,
        contact: bool,
        standing_long_jump: bool,
        action_count: usize,
    },
    JumpPosition {
        velocity: [f32; 3],
        position: WirePosition,
    },
    NonAutonomousJump,
    Jump {
        velocity: [f32; 3],
        object: u32,
        spell: u32,
    },
    Position {
        position: WirePosition,
        contact: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DispatchedLocomotion {
    pub context: ActionContext,
    pub epochs: Option<MovementEpochs>,
    pub request: LocomotionRequest,
    pub observations: LocomotionObservations,
}
pub fn decode_locomotion(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    limit: usize,
) -> Result<DispatchedLocomotion, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope = GameActionEnvelope::decode(bytes, limit).map_err(DispatchError::Wire)?;
    let (epochs, request, observations) = match envelope.action {
        GameActionType::MoveToState => {
            move_to_state(envelope.payload, limit).map(|(e, r, o)| (Some(e), r, o))
        }
        GameActionType::Jump => jump_variant(envelope.payload, limit),
        GameActionType::JumpNonAutonomous => {
            bace_wire::ClientNonAutonomousJump::decode(envelope.payload, limit).and_then(|jump| {
                if jump.extent.is_finite() {
                    Ok((
                        None,
                        LocomotionRequest::Jump {
                            extent: jump.extent,
                        },
                        LocomotionObservations::NonAutonomousJump,
                    ))
                } else {
                    Err(WireError::InvalidEncoding)
                }
            })
        }
        GameActionType::AutonomousPosition => {
            autonomous_position(envelope.payload, limit).map(|(e, r, o)| (Some(e), r, o))
        }
        other => return Err(DispatchError::UnsupportedAction(other.0)),
    }
    .map_err(DispatchError::Wire)?;
    binding.sequence = envelope.sequence;
    Ok(DispatchedLocomotion {
        context: binding,
        epochs,
        request,
        observations,
    })
}
/// Epochs are compared by the canonical replica owner before bounded submission;
/// the simulation independently rechecks the current Body teleport epoch.
pub fn movement_epochs_match(observed: MovementEpochs, current: MovementEpochs) -> bool {
    observed == current
}
fn move_to_state(
    bytes: &[u8],
    limit: usize,
) -> Result<(MovementEpochs, LocomotionRequest, LocomotionObservations), WireError> {
    let input = ClientMoveToState::decode(bytes, limit, 32)?;
    if input.trailing_bytes != 0
        || input.contact_long_jump & !3 != 0
        || !finite_position(input.reported_position)
        || input
            .motion
            .commands
            .iter()
            .any(|a| !a.speed.is_finite() || a.speed.abs() > 20.0)
    {
        return Err(WireError::InvalidEncoding);
    }
    let observations = LocomotionObservations::State {
        position: input.reported_position,
        contact: input.reported_contact(),
        standing_long_jump: input.reported_standing_long_jump(),
        action_count: input.motion.commands.len(),
    };
    let raw = input.motion;
    // Only the separate combat/cast owner may issue trusted action hooks.
    Ok((
        input.epochs,
        LocomotionRequest::State(RawLocomotionState {
            style: raw.current_style.unwrap_or(0x8000003d),
            current_hold: raw.current_hold_key.unwrap_or(1),
            forward: (
                raw.forward_command.unwrap_or(0x41000003),
                raw.forward_hold_key.unwrap_or(0),
                raw.forward_speed.unwrap_or(1.0),
            ),
            sidestep: (
                raw.sidestep_command.unwrap_or(0),
                raw.sidestep_hold_key.unwrap_or(0),
                raw.sidestep_speed.unwrap_or(1.0),
            ),
            turn: (
                raw.turn_command.unwrap_or(0),
                raw.turn_hold_key.unwrap_or(0),
                raw.turn_speed.unwrap_or(1.0),
            ),
        }),
        observations,
    ))
}
fn jump_variant(
    bytes: &[u8],
    limit: usize,
) -> Result<
    (
        Option<MovementEpochs>,
        LocomotionRequest,
        LocomotionObservations,
    ),
    WireError,
> {
    if bytes.len() == 32 {
        return jump(bytes, limit).map(|(e, r, o)| (Some(e), r, o));
    }
    let jump = bace_wire::ClientPositionJump::decode(bytes, limit)?;
    if !jump.extent.is_finite()
        || !jump.reported_velocity.into_iter().all(f32::is_finite)
        || !finite_position(jump.reported_position)
    {
        return Err(WireError::InvalidEncoding);
    }
    Ok((
        Some(jump.epochs),
        LocomotionRequest::Jump {
            extent: jump.extent,
        },
        LocomotionObservations::JumpPosition {
            velocity: jump.reported_velocity,
            position: jump.reported_position,
        },
    ))
}
fn jump(
    bytes: &[u8],
    limit: usize,
) -> Result<(MovementEpochs, LocomotionRequest, LocomotionObservations), WireError> {
    let input = ClientJump::decode(bytes, limit)?;
    if input.trailing_bytes != 0
        || !input.extent.is_finite()
        || !input.reported_velocity.into_iter().all(f32::is_finite)
    {
        return Err(WireError::InvalidEncoding);
    }
    Ok((
        input.epochs,
        LocomotionRequest::Jump {
            extent: input.extent,
        },
        LocomotionObservations::Jump {
            velocity: input.reported_velocity,
            object: input.object_id,
            spell: input.spell_id,
        },
    ))
}
fn autonomous_position(
    bytes: &[u8],
    limit: usize,
) -> Result<(MovementEpochs, LocomotionRequest, LocomotionObservations), WireError> {
    let input = ClientAutonomousPosition::decode(bytes, limit)?;
    if input.trailing_bytes != 0 || !finite_position(input.reported_position) {
        return Err(WireError::InvalidEncoding);
    }
    Ok((
        input.epochs,
        LocomotionRequest::ObservePosition,
        LocomotionObservations::Position {
            position: input.reported_position,
            contact: input.reported_contact,
        },
    ))
}
fn finite_position(p: WirePosition) -> bool {
    p.origin.into_iter().chain(p.rotation).all(f32::is_finite)
}
