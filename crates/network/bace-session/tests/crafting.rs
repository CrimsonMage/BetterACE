use bace_gameplay_api::{ActionContext, SessionId};
use bace_session::{DispatchError, SessionState, decode_crafting};
use bace_types::{AccountId, EntityId};
use bace_wire::{CraftingAction, GameActionEnvelope, opcode::GameActionType};
#[test]
fn crafting_dispatch_binds_identity_and_scopes_confirmation_type() {
    let binding = ActionContext {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(3),
        sequence: 0,
    };
    let payload = [5u32.to_le_bytes(), 17u32.to_le_bytes(), 1u32.to_le_bytes()].concat();
    let bytes = GameActionEnvelope {
        sequence: 44,
        action: GameActionType::ConfirmationResponse,
        payload: &payload,
    }
    .encode(128)
    .unwrap();
    let request = decode_crafting(SessionState::WorldConnected, binding, &bytes, 128).unwrap();
    assert_eq!(
        request.context,
        ActionContext {
            sequence: 44,
            ..binding
        }
    );
    assert_eq!(
        request.request,
        CraftingAction::Confirmation {
            confirmation_type: 5,
            context: 17,
            accepted: true
        }
    );
    assert_eq!(
        decode_crafting(SessionState::AuthConnected, binding, &bytes, 128),
        Err(DispatchError::WrongState)
    );
    let mut other = bytes;
    other[12..16].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        decode_crafting(SessionState::WorldConnected, binding, &other, 128),
        Err(DispatchError::InvalidTarget(2))
    );
}
