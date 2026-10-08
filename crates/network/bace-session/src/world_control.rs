//! World-only controls retain authenticated identity and optional action sequence.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::CharacterBinding;
use bace_wire::{WireError, WorldControlRequest};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchedWorldControl {
    pub binding: CharacterBinding,
    pub request: WorldControlRequest,
}
pub fn decode_world_control(
    state: SessionState,
    binding: CharacterBinding,
    bytes: &[u8],
    max_message_bytes: usize,
) -> Result<DispatchedWorldControl, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let request =
        WorldControlRequest::decode(bytes, max_message_bytes).map_err(|error| match error {
            WireError::UnexpectedOpcode(op) => DispatchError::UnsupportedAction(op),
            other => DispatchError::Wire(other),
        })?;
    Ok(DispatchedWorldControl { binding, request })
}
