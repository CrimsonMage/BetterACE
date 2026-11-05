use super::*;

#[test]
fn npc_use_preserves_reliable_message_order_when_payload_counter_differs() {
    // ACE GameActionUseItem reads only the target after the common GameAction
    // header. The payload counter is parsed, but character replay authority
    // follows the ordered transport message shared with F61C movement.
    let target = 0x8005_7d40u32;
    let bytes = bace_wire::GameActionEnvelope {
        sequence: 1,
        action: bace_wire::opcode::GameActionType::Use,
        payload: &target.to_le_bytes(),
    }
    .encode(1024)
    .unwrap();
    let message = bace_transport::ReceivedMessage {
        sequence: 55,
        id: 1,
        queue: 3,
        bytes,
    };
    let envelope = bace_wire::GameActionEnvelope::decode(&message.bytes, 1024).unwrap();
    assert_eq!(envelope.sequence, 1);
    assert_eq!(
        bace_wire::InventoryRequest::decode(envelope.action, envelope.payload, 1024, 0)
            .unwrap()
            .action,
        bace_wire::InventoryAction::Use(target)
    );
    let binding = CharacterBinding {
        actor: bace_types::EntityId(0x5000_0001),
        account: bace_types::AccountId(7),
        session: bace_gameplay_api::SessionId(9),
    };
    let context = player_context(binding, &message);
    assert_eq!(context.sequence, 55);
    assert_eq!(context.actor, binding.actor);
    assert_eq!(context.account, binding.account);
    assert_eq!(context.session, binding.session);
}
