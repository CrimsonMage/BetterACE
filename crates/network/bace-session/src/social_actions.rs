//! Authenticated dispatch only; claims in chat payloads never replace the binding.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::{
    ActionContext,
    corpse_consent::CorpseConsentRequest,
    social::{AllegianceRequest, FellowshipRequest, SocialRequest},
};
use bace_types::EntityId;
use bace_wire::{GameActionEnvelope, GroupAction as G, GroupRequest, SocialAction as S};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialDispatch {
    Social(SocialRequest),
    CorpseConsent(CorpseConsentRequest),
    Fellowship(FellowshipRequest),
    Allegiance(AllegianceRequest),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchedSocial {
    pub context: ActionContext,
    pub request: SocialDispatch,
    pub trailing_bytes: usize,
}
pub fn decode_social_action(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    max_payload: usize,
    max_string: usize,
) -> Result<DispatchedSocial, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let e = GameActionEnvelope::decode(bytes, max_payload).map_err(DispatchError::Wire)?;
    binding.sequence = e.sequence;
    match bace_wire::SocialRequest::decode(e.action, e.payload, max_payload, max_string) {
        Ok(input) => Ok(DispatchedSocial {
            context: binding,
            request: social(input.action),
            trailing_bytes: input.trailing_bytes,
        }),
        Err(bace_wire::WireError::UnexpectedOpcode(_)) => {
            let input = GroupRequest::decode(e.action, e.payload, max_payload, max_string)
                .map_err(DispatchError::Wire)?;
            Ok(DispatchedSocial {
                context: binding,
                request: group(input.action)?,
                trailing_bytes: input.trailing_bytes,
            })
        }
        Err(error) => Err(DispatchError::Wire(error)),
    }
}
/// Turbine messages lack the ordinary game-action sequence. The trusted session
/// adapter must allocate a monotonic action sequence in `binding` before dispatch.
pub fn decode_turbine_social(
    state: SessionState,
    binding: ActionContext,
    bytes: &[u8],
    max_payload: usize,
    max_string: usize,
) -> Result<DispatchedSocial, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let input = bace_wire::TurbineChatRequest::decode(bytes, max_payload, max_string)
        .map_err(DispatchError::Wire)?;
    Ok(DispatchedSocial {
        context: binding,
        trailing_bytes: input.trailing_bytes,
        request: SocialDispatch::Social(SocialRequest::Turbine {
            context_id: input.context_id,
            dispatch: input.header.dispatch,
            channel: input.channel,
            chat_type: input.chat_type,
            text: input.text,
        }),
    })
}
fn social(input: S) -> SocialDispatch {
    use CorpseConsentRequest as C;
    use SocialDispatch as D;
    use SocialRequest as R;
    match input {
        S::Talk(v) => D::Social(R::Talk(v)),
        S::Tell { text, target_name } => D::Social(R::Tell { text, target_name }),
        S::TalkDirect { text, target_id } => D::Social(R::TalkDirect {
            text,
            target: EntityId(target_id),
        }),
        S::ChatChannel { channel, text } => D::Social(R::Channel { channel, text }),
        S::Emote(v) => D::Social(R::Emote(v)),
        S::SoulEmote(v) => D::Social(R::SoulEmote(v)),
        S::SetAfkMode(v) => D::Social(R::SetAfk(v)),
        S::SetAfkMessage(v) => D::Social(R::SetAfkMessage(v)),
        S::AddFriend(v) => D::Social(R::AddFriend(v.trim().to_owned())),
        S::RemoveFriend(v) => D::Social(R::RemoveFriend(EntityId(v))),
        S::RemoveAllFriends => D::Social(R::RemoveAllFriends),
        S::AddChannel(v) => D::Social(R::AddChannel(v)),
        S::RemoveChannel(v) => D::Social(R::RemoveChannel(v)),
        S::ModifyGlobalSquelch {
            enabled,
            message_type,
        } => D::Social(R::GlobalSquelch {
            enabled,
            message_type,
        }),
        S::ModifyCharacterSquelch {
            enabled,
            object_id,
            name,
            message_type,
        } => D::Social(R::CharacterSquelch {
            enabled,
            target: EntityId(object_id),
            name,
            message_type,
        }),
        S::ModifyAccountSquelch { enabled, name } => D::Social(R::AccountSquelch { enabled, name }),
        S::ClearPlayerConsentList => D::CorpseConsent(C::Clear),
        S::DisplayPlayerConsentList => D::CorpseConsent(C::Display),
        S::RemoveFromPlayerConsentList(name) => D::CorpseConsent(C::RemoveFrom(name)),
        S::AddPlayerPermission(name) => D::CorpseConsent(C::Add(name)),
        S::RemovePlayerPermission(name) => D::CorpseConsent(C::Remove(name)),
    }
}

fn group(input: G) -> Result<SocialDispatch, DispatchError> {
    use AllegianceRequest as A;
    use FellowshipRequest as F;
    let result = match input {
        G::FellowCreate { name, share_xp } => {
            SocialDispatch::Fellowship(F::Create { name, share_xp })
        }
        G::FellowRecruit(v) => SocialDispatch::Fellowship(F::Recruit(EntityId(v))),
        G::FellowQuit(disband) => SocialDispatch::Fellowship(F::Quit { disband }),
        G::FellowDismiss(v) => SocialDispatch::Fellowship(F::Dismiss(EntityId(v))),
        G::FellowLeader(v) => SocialDispatch::Fellowship(F::AssignLeader(EntityId(v))),
        G::FellowOpen(v) => SocialDispatch::Fellowship(F::ChangeOpenness(v)),
        G::FellowPanel(v) => SocialDispatch::Fellowship(F::Panel(v)),
        G::Confirm {
            kind: 4,
            token,
            accepted,
        } => SocialDispatch::Fellowship(F::Confirm { token, accepted }),
        G::Confirm {
            kind: 1,
            token,
            accepted,
        } => SocialDispatch::Allegiance(A::Confirm { token, accepted }),
        G::Confirm { kind, .. } => return Err(DispatchError::InvalidTarget(kind)),
        value => SocialDispatch::Allegiance(match value {
            G::Swear(v) => A::Swear(EntityId(v)),
            G::Break(v) => A::Break(EntityId(v)),
            G::AllegiancePanel(enabled) => A::Update { enabled },
            G::Info(v) => A::Info(v),
            G::QueryMotd => A::QueryMotd,
            G::SetMotd(v) => A::SetMotd(v),
            G::ClearMotd => A::ClearMotd,
            G::QueryName => A::QueryName,
            G::SetName(v) => A::SetName(v),
            G::ClearName => A::ClearName,
            G::ListOfficers => A::ListOfficers,
            G::SetOfficer { name, level } => A::SetOfficer { name, level },
            G::RemoveOfficer(v) => A::RemoveOfficer(v),
            G::ClearOfficers => A::ClearOfficers,
            G::ListTitles => A::ListOfficerTitles,
            G::SetTitle { level, title } => A::SetOfficerTitle { level, title },
            G::ClearTitles => A::ClearOfficerTitles,
            G::Lock(v) => A::Lock(v),
            G::Approve(v) => A::ApproveVassal(v),
            G::ChatBoot { name, reason } => A::ChatBoot { name, reason },
            G::ChatGag { name, enabled } => A::ChatGag { name, enabled },
            G::ListBans => A::ListBans,
            G::AddBan(v) => A::AddBan(v),
            G::RemoveBan(v) => A::RemoveBan(v),
            G::Boot { name, account } => A::Boot { name, account },
            G::House(v) => A::HouseAction(v),
            G::Recall => A::RecallHometown,
            _ => return Err(DispatchError::UnsupportedAction(0)),
        }),
    };
    Ok(result)
}
