use super::*;
#[test]
fn independent_game_action_counter_cannot_rewind_or_exhaust_reliable_authority() {
    let context = ActionContext {
        actor: bace_types::EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
        sequence: 107,
    };
    for inner in [0, 1, 107, 108, u32::MAX] {
        let bytes: [u8; 12] = [0xf7b1u32, inner, 0x28d]
            .map(u32::to_le_bytes)
            .concat()
            .try_into()
            .unwrap();
        let action = decode_authenticated_recall(context, &bytes, 1024).unwrap();
        assert_eq!(action.context, context);
        assert_eq!(action.request, bace_wire::RecallAction::Marketplace);
    }
}
