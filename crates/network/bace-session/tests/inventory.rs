use bace_gameplay_api::{ActionContext, InventoryRequest, SessionId};
use bace_session::{SessionState, decode_inventory};
use bace_types::{AccountId, EntityId};
#[test]
fn split_wire_preserves_bound_identity_and_rejects_truncation_before_dispatch() {
    // Pinned ACE GameAction envelope F7B1 + StackableSplitToContainer(0x55),
    // independently serialized layouts are covered by bace-wire inventory goldens.
    let mut bytes = Vec::new();
    for word in [0xf7b1u32, 37, 0x55, 0x80000001, 0x50000001, 2, 3] {
        bytes.extend(word.to_le_bytes());
    }
    let binding = ActionContext {
        session: SessionId(7),
        account: AccountId(8),
        actor: EntityId(0x50000001),
        sequence: 0,
    };
    let got = decode_inventory(SessionState::WorldConnected, binding, &bytes, 128).unwrap();
    assert_eq!(got.context.sequence, 37);
    assert_eq!(got.context.actor, binding.actor);
    assert_eq!(got.context.account, binding.account);
    assert_eq!(
        got.request,
        InventoryRequest::SplitToContainer {
            item: EntityId(0x80000001),
            container: binding.actor,
            placement: 2,
            amount: 3
        }
    );
    for length in 0..bytes.len() {
        assert!(
            decode_inventory(SessionState::WorldConnected, binding, &bytes[..length], 128).is_err()
        );
    }
    assert!(decode_inventory(SessionState::AuthConnected, binding, &bytes, 128).is_err());
}
