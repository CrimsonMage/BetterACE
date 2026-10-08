use bace_gameplay_api::{ActionContext, CombatRequest, SessionId};
use bace_session::{DispatchError, SessionState, decode_combat};
use bace_types::{AccountId, EntityId};
#[test]
fn dispatch_binds_authenticated_actor_and_preserves_target_as_untrusted_request() {
    let binding = ActionContext {
        session: SessionId(7),
        account: AccountId(8),
        actor: EntityId(9),
        sequence: 0,
    };
    let mut bytes = Vec::new();
    for word in [0xf7b1u32, 42, 8, 0x80000001, 2, 0.5f32.to_bits()] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.push(0xa5);
    let decoded = decode_combat(SessionState::WorldConnected, binding, &bytes, 128).unwrap();
    assert_eq!(
        decoded.context,
        ActionContext {
            sequence: 42,
            ..binding
        }
    );
    assert_eq!(
        decoded.request,
        CombatRequest::TargetedMelee {
            target: EntityId(0x80000001),
            height: 2,
            power: 0.5
        }
    );
    assert_eq!(decoded.ignored_trailing_bytes, 1);
    assert_eq!(
        decode_combat(SessionState::AuthConnected, binding, &bytes, 128),
        Err(DispatchError::WrongState)
    );
    assert!(decode_combat(SessionState::WorldConnected, binding, &bytes[..23], 128).is_err());
}
#[test]
fn portal_exit_is_a_world_observation_and_never_an_auth_state_entry_trigger() {
    let binding = bace_gameplay_api::CharacterBinding {
        session: SessionId(7),
        account: AccountId(8),
        actor: EntityId(9),
    };
    let bytes = [0xb1, 0xf7, 0, 0, 42, 0, 0, 0, 0xa1, 0, 0, 0];
    assert_eq!(
        bace_session::decode_world_control(SessionState::AuthConnected, binding, &bytes, 128),
        Err(DispatchError::WrongState)
    );
    let result =
        bace_session::decode_world_control(SessionState::WorldConnected, binding, &bytes, 128)
            .unwrap();
    assert_eq!(result.binding, binding);
    assert_eq!(result.request.action_sequence, Some(42));
}
