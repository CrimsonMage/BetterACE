use bace_gameplay_api::{ActionContext, CastRequest, SessionId};
use bace_session::{DispatchError, SessionState, decode_magic};
use bace_types::{AccountId, EntityId};
#[test]
fn casts_use_authenticated_caster_and_never_payload_identity() {
    let binding = ActionContext {
        session: SessionId(1),
        account: AccountId(2),
        actor: EntityId(3),
        sequence: 0,
    };
    let bytes: [u8; 20] = [
        0xb1, 0xf7, 0, 0, 9, 0, 0, 0, 0x4a, 0, 0, 0, 8, 0, 0, 0, 7, 0, 0, 0,
    ];
    let result = decode_magic(SessionState::WorldConnected, binding, &bytes, 32).unwrap();
    assert_eq!(result.context.actor, EntityId(3));
    assert_eq!(result.context.sequence, 9);
    assert_eq!(
        result.request,
        CastRequest::Targeted {
            target: EntityId(8),
            spell: 7
        }
    );
    assert_eq!(
        decode_magic(SessionState::AuthConnected, binding, &bytes, 32),
        Err(DispatchError::WrongState)
    );
    assert!(decode_magic(SessionState::WorldConnected, binding, &bytes[..19], 32).is_err());
}
