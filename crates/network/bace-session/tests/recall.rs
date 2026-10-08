use bace_gameplay_api::{ActionContext, SessionId};
use bace_session::{DispatchError, SessionState, decode_recall};
use bace_types::{AccountId, EntityId};
use bace_wire::{GameActionEnvelope, RecallAction, opcode::GameActionType};
#[test]
fn recalls_use_authenticated_identity_and_wire_sequence_only_after_world_admission() {
    let binding = ActionContext {
        actor: EntityId(7),
        account: AccountId(8),
        session: SessionId(9),
        sequence: 0,
    };
    let bytes = GameActionEnvelope {
        sequence: 123,
        action: GameActionType::TeleToMarketPlace,
        payload: &[1, 2, 3, 4],
    }
    .encode(4)
    .unwrap();
    let request = decode_recall(SessionState::WorldConnected, binding, &bytes, 4).unwrap();
    assert_eq!(
        request.context,
        ActionContext {
            sequence: 123,
            ..binding
        }
    );
    assert_eq!(request.request, RecallAction::Marketplace);
    assert_eq!(request.ignored_trailing_bytes, 4);
    assert_eq!(
        decode_recall(SessionState::AuthConnected, binding, &bytes, 4),
        Err(DispatchError::WrongState)
    );
    for length in 0..12 {
        assert!(decode_recall(SessionState::WorldConnected, binding, &bytes[..length], 4).is_err());
    }
    assert!(decode_recall(SessionState::WorldConnected, binding, &bytes, 3).is_err());
}
