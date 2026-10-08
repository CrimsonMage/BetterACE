//! Query decoding binds authenticated identity and leaves target validation to simulation.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::{ActionContext, selection::TargetQueryKind};
use bace_types::EntityId;
pub fn decode_target_query(
    state: SessionState,
    mut context: ActionContext,
    bytes: &[u8],
    maximum: usize,
) -> Result<(ActionContext, TargetQueryKind, EntityId), DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope =
        bace_wire::GameActionEnvelope::decode(bytes, maximum).map_err(DispatchError::Wire)?;
    let (kind, target) =
        match bace_wire::TargetQueryInput::decode(envelope.action, envelope.payload, maximum)
            .map_err(DispatchError::Wire)?
        {
            bace_wire::TargetQueryInput::Health(id) => (TargetQueryKind::Health, id),
            bace_wire::TargetQueryInput::ItemMana(id) => (TargetQueryKind::ItemMana, id),
        };
    context.sequence = envelope.sequence;
    Ok((context, kind, EntityId(target)))
}
