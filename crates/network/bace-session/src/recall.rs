//! Bind zero-field recall actions to the authenticated world session. Geometry,
//! permissions, animation and durable destination changes remain server-owned.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::ActionContext;
use bace_wire::{GameActionEnvelope, RecallAction, RecallRequest, WireError};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchedRecall {
    pub context: ActionContext,
    pub request: RecallAction,
    pub ignored_trailing_bytes: usize,
}
pub fn decode_recall(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    maximum: usize,
) -> Result<DispatchedRecall, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope = GameActionEnvelope::decode(bytes, maximum).map_err(DispatchError::Wire)?;
    let request =
        RecallRequest::decode(envelope.action, envelope.payload, maximum).map_err(|e| match e {
            WireError::UnexpectedOpcode(op) => DispatchError::UnsupportedAction(op),
            other => DispatchError::Wire(other),
        })?;
    binding.sequence = envelope.sequence;
    Ok(DispatchedRecall {
        context: binding,
        request: request.action,
        ignored_trailing_bytes: request.trailing_bytes,
    })
}
