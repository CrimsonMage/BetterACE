//! Authenticated social ingress. Pinned GameActionTalk intercepts only '@';
//! commands never enter ordinary/public chat or bypass live role authorization.
use bace_gameplay_api::{
    ActionContext, corpse_consent::CorpseConsentRequest, social::SocialRequest,
    staff::MapTeleportRequest,
};
use bace_session::{DispatchedSocial, SessionState, SocialDispatch};
use bace_simulation::Command;
pub enum GameplayDispatch {
    TargetQuery {
        context: ActionContext,
        kind: bace_gameplay_api::selection::TargetQueryKind,
        target: bace_types::EntityId,
    },
    SocialLookup {
        context: ActionContext,
        request: SocialRequest,
    },
    CorpseConsent {
        context: ActionContext,
        request: CorpseConsentRequest,
    },
    Simulation(Box<Command>),
    StaffLine {
        context: ActionContext,
        line: String,
    },
    Map {
        context: ActionContext,
        request: MapTeleportRequest,
    },
}
pub fn decode_social_gameplay(
    state: SessionState,
    binding: ActionContext,
    bytes: &[u8],
    max_payload: usize,
    max_string: usize,
) -> Result<GameplayDispatch, bace_session::DispatchError> {
    let envelope = bace_wire::GameActionEnvelope::decode(bytes, max_payload)
        .map_err(bace_session::DispatchError::Wire)?;
    if envelope.action == bace_wire::opcode::GameActionType::AdvocateTeleport {
        let (context, request) =
            bace_session::decode_staff_map(state, binding, bytes, max_payload, max_string)?;
        return Ok(GameplayDispatch::Map { context, request });
    }
    if matches!(
        envelope.action,
        bace_wire::opcode::GameActionType::QueryHealth
            | bace_wire::opcode::GameActionType::QueryItemMana
    ) {
        let (context, kind, target) =
            bace_session::decode_target_query(state, binding, bytes, max_payload)?;
        return Ok(GameplayDispatch::TargetQuery {
            context,
            kind,
            target,
        });
    }
    Ok(route(bace_session::decode_social_action(
        state,
        binding,
        bytes,
        max_payload,
        max_string,
    )?))
}
pub fn decode_turbine_gameplay(
    state: SessionState,
    binding: ActionContext,
    bytes: &[u8],
    max_payload: usize,
    max_string: usize,
) -> Result<GameplayDispatch, bace_session::DispatchError> {
    Ok(route(bace_session::decode_turbine_social(
        state,
        binding,
        bytes,
        max_payload,
        max_string,
    )?))
}
fn route(input: DispatchedSocial) -> GameplayDispatch {
    let context = input.context;
    let command = match input.request {
        SocialDispatch::Social(SocialRequest::Talk(text)) if text.starts_with('@') => {
            return GameplayDispatch::StaffLine {
                context,
                line: text,
            };
        }
        SocialDispatch::Social(
            request @ (SocialRequest::AddFriend(_)
            | SocialRequest::CharacterSquelch { .. }
            | SocialRequest::AccountSquelch { .. }),
        ) => return GameplayDispatch::SocialLookup { context, request },
        SocialDispatch::Social(request) => Command::Social { context, request },
        SocialDispatch::CorpseConsent(request) => {
            return GameplayDispatch::CorpseConsent { context, request };
        }
        SocialDispatch::Fellowship(request) => Command::Fellowship { context, request },
        SocialDispatch::Allegiance(request) => Command::Allegiance { context, request },
    };
    GameplayDispatch::Simulation(Box::new(command))
}

impl std::fmt::Debug for GameplayDispatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetQuery {
                context,
                kind,
                target,
            } => f
                .debug_tuple("TargetQuery")
                .field(context)
                .field(kind)
                .field(target)
                .finish(),
            Self::SocialLookup { context, .. } => {
                f.debug_tuple("SocialLookup").field(context).finish()
            }
            Self::CorpseConsent { context, request } => f
                .debug_struct("CorpseConsent")
                .field("context", context)
                .field("request", request)
                .finish(),
            Self::Simulation(_) => f.write_str("Simulation(..)"),
            Self::StaffLine { context, .. } => f
                .debug_struct("StaffLine")
                .field("context", context)
                .field("line", &"[REDACTED]")
                .finish(),
            Self::Map { context, request } => f
                .debug_struct("Map")
                .field("context", context)
                .field("request", request)
                .finish(),
        }
    }
}
