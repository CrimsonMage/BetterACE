use bace_gameplay_api::{ActionContext, SessionId, UseDoor};
use bace_session::{DispatchError, SessionState, decode_door_use};
use bace_types::{AccountId, EntityId};
#[test]
fn door_use_retains_server_binding_and_rejects_other_use_families() {
    let context = ActionContext {
        session: SessionId(3),
        account: AccountId(4),
        actor: EntityId(5),
        sequence: 0,
    };
    let mut bytes = Vec::new();
    for word in [0xf7b1u32, 42, 0x36, 0x80000001] {
        bytes.extend(word.to_le_bytes())
    }
    bytes.push(0xa5);
    let result = decode_door_use(SessionState::WorldConnected, context, &bytes, 128).unwrap();
    assert_eq!(
        result.context,
        ActionContext {
            sequence: 42,
            ..context
        }
    );
    assert_eq!(
        result.request,
        UseDoor {
            door: EntityId(0x80000001)
        }
    );
    assert_eq!(result.ignored_trailing_bytes, 1);
    assert_eq!(
        decode_door_use(SessionState::AuthConnected, context, &bytes, 128),
        Err(DispatchError::WrongState)
    );
    bytes[8] = 0x35;
    assert_eq!(
        decode_door_use(SessionState::WorldConnected, context, &bytes, 128),
        Err(DispatchError::UnsupportedAction(0x35))
    );
}
