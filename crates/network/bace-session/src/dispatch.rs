//! Decode the implemented action slice without trusting actor/account fields
//! from clients. Unsupported actions remain errors, never successful no-ops.
use crate::SessionState;
use bace_gameplay_api::{ActionContext, AttributeId, ProgressionTarget, RaiseProgression, VitalId};
use bace_wire::{GameActionEnvelope, ProgressionAction, ProgressionRequest, WireError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchError {
    WrongState,
    Wire(WireError),
    UnsupportedAction(u32),
    InvalidTarget(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchedProgression {
    pub context: ActionContext,
    pub request: RaiseProgression,
    pub ignored_trailing_bytes: usize,
}
/// `binding` is supplied by the authenticated session owner; its sequence is
/// replaced by the decoded action sequence. Simulation rechecks ownership and
/// replay before any mutation, so decoding itself grants no progression.
pub fn decode_progression(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    max_payload_bytes: usize,
) -> Result<DispatchedProgression, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope =
        GameActionEnvelope::decode(bytes, max_payload_bytes).map_err(DispatchError::Wire)?;
    let decoded = ProgressionRequest::decode(envelope.action, envelope.payload, max_payload_bytes)
        .map_err(|error| match error {
            WireError::UnexpectedOpcode(op) => DispatchError::UnsupportedAction(op),
            other => DispatchError::Wire(other),
        })?;
    let (target, amount) = match decoded.action {
        ProgressionAction::RaiseAttribute {
            attribute,
            experience_spent,
        } => (
            ProgressionTarget::Attribute(
                AttributeId::try_from(attribute)
                    .map_err(|_| DispatchError::InvalidTarget(attribute))?,
            ),
            experience_spent,
        ),
        ProgressionAction::RaiseVital {
            vital,
            experience_spent,
        } => (
            ProgressionTarget::Vital(
                VitalId::try_from(vital).map_err(|_| DispatchError::InvalidTarget(vital))?,
            ),
            experience_spent,
        ),
        ProgressionAction::RaiseSkill {
            skill,
            experience_spent,
        } => (ProgressionTarget::Skill(skill), experience_spent),
        ProgressionAction::TrainSkill { .. } => {
            return Err(DispatchError::UnsupportedAction(envelope.action.0));
        }
    };
    binding.sequence = envelope.sequence;
    Ok(DispatchedProgression {
        context: binding,
        request: RaiseProgression { target, amount },
        ignored_trailing_bytes: decoded.trailing_bytes,
    })
}
