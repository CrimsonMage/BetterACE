//! Staff map action decoding grants no privilege and does not accept a pose.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::{ActionContext, staff::MapTeleportRequest};
use bace_wire::opcode::GameActionType;
use bace_wire::{GameActionEnvelope, MapTeleportInput};
pub fn decode_staff_map(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    max_payload: usize,
    max_string: usize,
) -> Result<(ActionContext, MapTeleportRequest), DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope = GameActionEnvelope::decode(bytes, max_payload).map_err(DispatchError::Wire)?;
    if envelope.action != GameActionType::AdvocateTeleport {
        return Err(DispatchError::UnsupportedAction(envelope.action.0));
    }
    let input = MapTeleportInput::decode(envelope.payload, max_payload, max_string)
        .map_err(DispatchError::Wire)?;
    binding.sequence = envelope.sequence;
    Ok((
        binding,
        MapTeleportRequest {
            cell: input.position.cell,
            origin: input.position.origin,
            rotation: input.position.rotation,
        },
    ))
}
