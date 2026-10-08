use crate::{DispatchError, SessionState};
use bace_gameplay_api::{ActionContext, CastRequest};
use bace_types::EntityId;
use bace_wire::{GameActionEnvelope, MagicAction, WireError};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchedMagic {
    pub context: ActionContext,
    pub request: CastRequest,
    pub ignored_trailing_bytes: usize,
}
pub fn decode_magic(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    max_payload_bytes: usize,
) -> Result<DispatchedMagic, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope =
        GameActionEnvelope::decode(bytes, max_payload_bytes).map_err(DispatchError::Wire)?;
    let decoded =
        bace_wire::MagicRequest::decode(envelope.action, envelope.payload, max_payload_bytes)
            .map_err(|error| match error {
                WireError::UnexpectedOpcode(op) => DispatchError::UnsupportedAction(op),
                other => DispatchError::Wire(other),
            })?;
    let request = match decoded.action {
        MagicAction::Targeted {
            target_id,
            spell_id,
        } => CastRequest::Targeted {
            target: EntityId(target_id),
            spell: spell_id,
        },
        MagicAction::Untargeted { spell_id } => CastRequest::Untargeted { spell: spell_id },
    };
    binding.sequence = envelope.sequence;
    Ok(DispatchedMagic {
        context: binding,
        request,
        ignored_trailing_bytes: decoded.trailing_bytes,
    })
}
