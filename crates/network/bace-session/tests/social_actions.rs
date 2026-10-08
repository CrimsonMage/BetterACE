use bace_gameplay_api::{
    ActionContext, SessionId, corpse_consent::CorpseConsentRequest, social::FellowshipRequest,
};
use bace_session::{SessionState, SocialDispatch, decode_social_action};
use bace_types::{AccountId, EntityId};
use bace_wire::{Writer, opcode::GameActionType};
#[test]
fn social_dispatch_retains_server_identity_and_never_routes_other_confirmation_types() {
    let binding = ActionContext {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(3),
        sequence: 0,
    };
    let mut w = Writer::new();
    for n in [0xf7b1, 7, GameActionType::ConfirmationResponse.0, 4, 99, 1] {
        w.u32(n)
    }
    let bytes = w.into_bytes();
    let result =
        decode_social_action(SessionState::WorldConnected, binding, &bytes, 128, 64).unwrap();
    assert_eq!(result.context.actor, binding.actor);
    assert_eq!(result.context.sequence, 7);
    assert_eq!(
        result.request,
        SocialDispatch::Fellowship(FellowshipRequest::Confirm {
            token: 99,
            accepted: true
        })
    );
    let mut wrong = bytes.clone();
    wrong[12..16].copy_from_slice(&5u32.to_le_bytes());
    assert!(decode_social_action(SessionState::WorldConnected, binding, &wrong, 128, 64).is_err());
    assert!(decode_social_action(SessionState::AuthConnected, binding, &bytes, 128, 64).is_err());
}

#[test]
fn corpse_consent_name_is_only_a_proposal_under_the_authenticated_binding() {
    let binding = ActionContext {
        actor: EntityId(41),
        account: AccountId(7),
        session: SessionId(99),
        sequence: 0,
    };
    let mut wire = Writer::new();
    for value in [0xf7b1, 123, GameActionType::AddPlayerPermission.0] {
        wire.u32(value);
    }
    wire.u16(3);
    wire.bytes(b"Ada");
    wire.align4();
    let bytes = wire.into_bytes();
    let request =
        decode_social_action(SessionState::WorldConnected, binding, &bytes, 128, 16).unwrap();
    assert_eq!(request.context.actor, binding.actor);
    assert_eq!(request.context.account, binding.account);
    assert_eq!(request.context.session, binding.session);
    assert_eq!(request.context.sequence, 123);
    assert_eq!(
        request.request,
        SocialDispatch::CorpseConsent(CorpseConsentRequest::Add("Ada".into()))
    );
    assert!(decode_social_action(SessionState::AuthConnected, binding, &bytes, 128, 16).is_err());
}
