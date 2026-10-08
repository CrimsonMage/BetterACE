//! Decode authenticated crafting intents; quotes and outcomes remain server-owned.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::ActionContext;
use bace_wire::{CraftingAction, CraftingRequest, GameActionEnvelope, WireError};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchedCrafting {
    pub context: ActionContext,
    pub request: CraftingAction,
    pub ignored_trailing_bytes: usize,
}
pub fn decode_crafting(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    max_payload_bytes: usize,
) -> Result<DispatchedCrafting, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope =
        GameActionEnvelope::decode(bytes, max_payload_bytes).map_err(DispatchError::Wire)?;
    let request = CraftingRequest::decode(envelope.action, envelope.payload, max_payload_bytes)
        .map_err(|e| match e {
            WireError::UnexpectedOpcode(op) => DispatchError::UnsupportedAction(op),
            other => DispatchError::Wire(other),
        })?;
    if let CraftingAction::Confirmation {
        confirmation_type, ..
    } = request.action
        && confirmation_type != 5
    {
        return Err(DispatchError::InvalidTarget(confirmation_type));
    }
    binding.sequence = envelope.sequence;
    Ok(DispatchedCrafting {
        context: binding,
        request: request.action,
        ignored_trailing_bytes: request.trailing_bytes,
    })
}
