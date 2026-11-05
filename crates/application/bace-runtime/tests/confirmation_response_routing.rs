//! ConfirmationResponse (0x0275) ingress routing. The social decoder runs before
//! the crafting and skill-device owners. Pinned ACE ConfirmationType.cs: 1 swear,
//! 2 AlterSkill, 3 AlterAttribute, 4 Fellowship, 5 CraftInteraction, 6 Augmentation,
//! 7 Yes_No. ACE HandleResponse routes each type to its own owner; a type the
//! social owner does not claim must yield UnsupportedAction (ingress continues),
//! never InvalidTarget (which the runtime turns into a session failure).
use bace_gameplay_api::{ActionContext, SessionId};
use bace_runtime::gameplay_dispatch::decode_social_gameplay;
use bace_session::{DispatchError, SessionState};
use bace_types::{AccountId, EntityId};
use bace_wire::{GameActionEnvelope, opcode::GameActionType};

const CONFIRMATION_RESPONSE: u32 = 0x0275;
const MAX_PAYLOAD: usize = 128;
const MAX_STRING: usize = 4096;

fn binding() -> ActionContext {
    ActionContext {
        session: SessionId(3),
        account: AccountId(2),
        actor: EntityId(1),
        sequence: 0,
    }
}

fn confirmation(kind: u32, token: u32, accepted: u32) -> Vec<u8> {
    let payload = [kind, token, accepted]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<u8>>();
    GameActionEnvelope {
        sequence: 7,
        action: GameActionType::ConfirmationResponse,
        payload: &payload,
    }
    .encode(MAX_PAYLOAD)
    .unwrap()
}

#[test]
fn social_decoder_claims_only_allegiance_and_fellowship_confirmations() {
    for kind in [1, 4] {
        let bytes = confirmation(kind, 99, 1);
        assert!(
            decode_social_gameplay(
                SessionState::WorldConnected,
                binding(),
                &bytes,
                MAX_PAYLOAD,
                MAX_STRING,
            )
            .is_ok(),
            "social must own confirmation kind {kind}"
        );
    }
    for kind in [0, 2, 3, 5, 6, 7] {
        let bytes = confirmation(kind, 99, 1);
        let result = decode_social_gameplay(
            SessionState::WorldConnected,
            binding(),
            &bytes,
            MAX_PAYLOAD,
            MAX_STRING,
        );
        assert!(
            matches!(
                result,
                Err(DispatchError::UnsupportedAction(CONFIRMATION_RESPONSE))
            ),
            "confirmation kind {kind} must fall through to its owner as Unsupported, got {result:?}"
        );
    }
}

#[test]
fn craft_and_device_confirmations_reach_their_owning_decoders() {
    let craft = confirmation(5, 99, 1);
    assert!(
        bace_session::decode_crafting(
            SessionState::WorldConnected,
            binding(),
            &craft,
            MAX_PAYLOAD,
        )
        .is_ok(),
        "crafting owner must decode CraftInteraction confirmation"
    );
    for kind in [2, 3, 6] {
        let device = confirmation(kind, 99, 1);
        assert!(
            bace_session::decode_skill_device(
                SessionState::WorldConnected,
                binding(),
                &device,
                MAX_PAYLOAD,
            )
            .is_ok(),
            "skill-device owner must decode confirmation kind {kind}"
        );
    }
}

#[test]
fn truncated_owned_confirmation_is_an_error_not_a_panic() {
    let mut bytes = confirmation(1, 99, 1);
    bytes.truncate(bytes.len() - 4);
    let result = decode_social_gameplay(
        SessionState::WorldConnected,
        binding(),
        &bytes,
        MAX_PAYLOAD,
        MAX_STRING,
    );
    assert!(
        result.is_err(),
        "a truncated fellowship/allegiance confirmation must be rejected"
    );
}
