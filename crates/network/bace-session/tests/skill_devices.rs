use bace_gameplay_api::{ActionContext, SessionId};
use bace_session::{
    DispatchError, SessionState, SkillDeviceAction, SkillDeviceConfirmationType,
    decode_skill_device,
};
use bace_types::{AccountId, EntityId};
use bace_wire::{GameActionEnvelope, opcode::GameActionType};
fn binding() -> ActionContext {
    ActionContext {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(3),
        sequence: 0,
    }
}
#[test]
fn use_and_official_device_confirmation_types_bind_authenticated_identity() {
    let payload = [10u32.to_le_bytes().as_slice(), &[0xaa]].concat();
    let bytes = GameActionEnvelope {
        action: GameActionType::Use,
        sequence: 7,
        payload: &payload,
    }
    .encode(128)
    .unwrap();
    let dispatched =
        decode_skill_device(SessionState::WorldConnected, binding(), &bytes, 128).unwrap();
    assert_eq!(dispatched.request, SkillDeviceAction::Use { item: 10 });
    assert_eq!(
        dispatched.context,
        ActionContext {
            sequence: 7,
            ..binding()
        }
    );
    assert_eq!(dispatched.ignored_trailing_bytes, 1);
    assert_eq!(
        decode_skill_device(SessionState::AuthConnected, binding(), &bytes, 128),
        Err(DispatchError::WrongState)
    );
    for (id, kind) in [
        (2, SkillDeviceConfirmationType::AlterSkill),
        (3, SkillDeviceConfirmationType::AlterAttribute),
        (6, SkillDeviceConfirmationType::Augmentation),
    ] {
        assert_eq!(kind.wire_id(), id);
        let payload = [
            id.to_le_bytes(),
            17u32.to_le_bytes(),
            u32::MAX.to_le_bytes(),
        ]
        .concat();
        let bytes = GameActionEnvelope {
            action: GameActionType::ConfirmationResponse,
            sequence: 8,
            payload: &payload,
        }
        .encode(128)
        .unwrap();
        let request =
            decode_skill_device(SessionState::WorldConnected, binding(), &bytes, 128).unwrap();
        assert_eq!(
            request.request,
            SkillDeviceAction::Confirmation {
                kind,
                token: 17,
                accepted: true
            }
        );
        assert_eq!(request.context.sequence, 8);
    }
    for id in [0u32, 1, 4, 5, 7, u32::MAX] {
        let payload = [id.to_le_bytes(), 17u32.to_le_bytes(), 1u32.to_le_bytes()].concat();
        let bytes = GameActionEnvelope {
            action: GameActionType::ConfirmationResponse,
            sequence: 8,
            payload: &payload,
        }
        .encode(128)
        .unwrap();
        assert_eq!(
            decode_skill_device(SessionState::WorldConnected, binding(), &bytes, 128),
            Err(DispatchError::InvalidTarget(id))
        );
    }
}
