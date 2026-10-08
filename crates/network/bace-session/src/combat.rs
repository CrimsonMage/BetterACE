//! Combat dispatch binds the actor to the authenticated session, never the payload.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::{ActionContext, CombatRequest};
use bace_types::EntityId;
use bace_wire::{CombatAction, GameActionEnvelope, WireError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DispatchedCombat {
    pub context: ActionContext,
    pub request: CombatRequest,
    pub ignored_trailing_bytes: usize,
}
pub fn decode_combat(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    max_payload_bytes: usize,
) -> Result<DispatchedCombat, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope =
        GameActionEnvelope::decode(bytes, max_payload_bytes).map_err(DispatchError::Wire)?;
    let decoded =
        bace_wire::CombatRequest::decode(envelope.action, envelope.payload, max_payload_bytes)
            .map_err(|error| match error {
                WireError::UnexpectedOpcode(op) => DispatchError::UnsupportedAction(op),
                other => DispatchError::Wire(other),
            })?;
    let request = match decoded.action {
        CombatAction::TargetedMelee {
            target_id,
            height,
            power,
        } => CombatRequest::TargetedMelee {
            target: EntityId(target_id),
            height,
            power,
        },
        CombatAction::TargetedMissile {
            target_id,
            height,
            accuracy,
        } => CombatRequest::TargetedMissile {
            target: EntityId(target_id),
            height,
            accuracy,
        },
        CombatAction::ChangeMode(mode) => CombatRequest::ChangeMode(mode),
        CombatAction::CancelAttack => CombatRequest::CancelAttack,
        CombatAction::QueryHealth(target_id) => CombatRequest::QueryHealth(EntityId(target_id)),
    };
    binding.sequence = envelope.sequence;
    Ok(DispatchedCombat {
        context: binding,
        request,
        ignored_trailing_bytes: decoded.trailing_bytes,
    })
}
