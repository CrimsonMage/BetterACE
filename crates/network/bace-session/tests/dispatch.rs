use bace_gameplay_api::*;
use bace_session::*;
use bace_types::{AccountId, EntityId};
#[test]
fn progression_dispatch_uses_server_identity_and_rejects_unimplemented_actions() {
    let binding = ActionContext {
        session: SessionId(7),
        account: AccountId(8),
        actor: EntityId(9),
        sequence: 0,
    };
    // F7B1, action sequence42, RaiseAttribute0045, Strength1, spentXP100.
    let bytes = [
        0xB1, 0xF7, 0, 0, 42, 0, 0, 0, 0x45, 0, 0, 0, 1, 0, 0, 0, 100, 0, 0, 0,
    ];
    let decoded = decode_progression(SessionState::WorldConnected, binding, &bytes, 1024).unwrap();
    assert_eq!(decoded.context.account, binding.account);
    assert_eq!(decoded.context.actor, binding.actor);
    assert_eq!(decoded.context.sequence, 42);
    assert_eq!(
        decoded.request,
        RaiseProgression {
            target: ProgressionTarget::Attribute(AttributeId::Strength),
            amount: 100
        }
    );
    assert_eq!(
        decode_progression(SessionState::AuthConnected, binding, &bytes, 1024),
        Err(DispatchError::WrongState)
    );
    let mut unsupported = bytes;
    unsupported[8] = 0x47;
    assert_eq!(
        decode_progression(SessionState::WorldConnected, binding, &unsupported, 1024),
        Err(DispatchError::UnsupportedAction(0x47))
    );
    let mut invalid = bytes;
    invalid[12] = 0xff;
    assert_eq!(
        decode_progression(SessionState::WorldConnected, binding, &invalid, 1024),
        Err(DispatchError::InvalidTarget(0xff))
    );
}
