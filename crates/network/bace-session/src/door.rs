//! UseItem dispatch selected by the server's object-kind router. Simulation must
//! recheck the target is a door, actor ownership, replay, reach and obstruction.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::{ActionContext, UseDoor};
use bace_types::EntityId;
use bace_wire::{
    GameActionEnvelope, InventoryAction, InventoryRequest, WireError, opcode::GameActionType,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchedDoorUse {
    pub context: ActionContext,
    pub request: UseDoor,
    pub ignored_trailing_bytes: usize,
}
pub fn decode_door_use(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    max_payload_bytes: usize,
) -> Result<DispatchedDoorUse, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope =
        GameActionEnvelope::decode(bytes, max_payload_bytes).map_err(DispatchError::Wire)?;
    if envelope.action != GameActionType::Use {
        return Err(DispatchError::UnsupportedAction(envelope.action.0));
    }
    let value = InventoryRequest::decode(envelope.action, envelope.payload, max_payload_bytes, 0)
        .map_err(DispatchError::Wire)?;
    let InventoryAction::Use(target) = value.action else {
        return Err(DispatchError::Wire(WireError::InvalidEncoding));
    };
    binding.sequence = envelope.sequence;
    Ok(DispatchedDoorUse {
        context: binding,
        request: UseDoor {
            door: EntityId(target),
        },
        ignored_trailing_bytes: value.trailing_bytes,
    })
}
