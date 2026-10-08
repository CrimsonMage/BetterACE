//! Pinned fellowship/allegiance action readers. Domain authorization is separate.
use crate::{Reader, WireError, opcode::GameActionType as Op};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupAction {
    FellowCreate {
        name: String,
        share_xp: bool,
    },
    FellowRecruit(u32),
    FellowQuit(bool),
    FellowDismiss(u32),
    FellowLeader(u32),
    FellowOpen(bool),
    FellowPanel(bool),
    Swear(u32),
    Break(u32),
    AllegiancePanel(bool),
    Info(String),
    QueryMotd,
    SetMotd(String),
    ClearMotd,
    QueryName,
    SetName(String),
    ClearName,
    ListOfficers,
    SetOfficer {
        name: String,
        level: u32,
    },
    RemoveOfficer(String),
    ClearOfficers,
    ListTitles,
    SetTitle {
        level: u32,
        title: String,
    },
    ClearTitles,
    Lock(u32),
    Approve(String),
    ChatBoot {
        name: String,
        reason: String,
    },
    ChatGag {
        name: String,
        enabled: bool,
    },
    ListBans,
    AddBan(String),
    RemoveBan(String),
    Boot {
        name: String,
        account: bool,
    },
    House(u32),
    Recall,
    Confirm {
        kind: u32,
        token: u32,
        accepted: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupRequest {
    pub action: GroupAction,
    pub trailing_bytes: usize,
}
impl GroupRequest {
    pub fn decode(
        op: Op,
        payload: &[u8],
        max_payload: usize,
        max_string: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload {
            return Err(WireError::LimitExceeded);
        }
        let mut r = Reader::new(payload);
        let action = match op {
            Op::FellowshipCreate => GroupAction::FellowCreate {
                name: r.client_string16(max_string)?,
                share_xp: r.u32()? != 0,
            },
            Op::FellowshipRecruit => GroupAction::FellowRecruit(r.u32()?),
            Op::FellowshipQuit => GroupAction::FellowQuit(r.u32()? != 0),
            Op::FellowshipDismiss => GroupAction::FellowDismiss(r.u32()?),
            Op::FellowshipAssignNewLeader => GroupAction::FellowLeader(r.u32()?),
            Op::FellowshipChangeOpenness => GroupAction::FellowOpen(r.u32()? != 0),
            Op::FellowshipUpdateRequest => GroupAction::FellowPanel(r.u32()? != 0),
            Op::SwearAllegiance => GroupAction::Swear(r.u32()?),
            Op::BreakAllegiance => GroupAction::Break(r.u32()?),
            Op::AllegianceUpdateRequest => GroupAction::AllegiancePanel(r.u32()? != 0),
            Op::AllegianceInfoRequest => GroupAction::Info(r.client_string16(max_string)?),
            Op::QueryMotd => GroupAction::QueryMotd,
            Op::SetMotd => GroupAction::SetMotd(r.client_string16(max_string)?),
            Op::ClearMotd => GroupAction::ClearMotd,
            Op::QueryAllegianceName => GroupAction::QueryName,
            Op::SetAllegianceName => GroupAction::SetName(r.client_string16(max_string)?),
            Op::ClearAllegianceName => GroupAction::ClearName,
            Op::ListAllegianceOfficers => GroupAction::ListOfficers,
            Op::SetAllegianceOfficer => GroupAction::SetOfficer {
                name: r.client_string16(max_string)?,
                level: r.u32()?,
            },
            Op::RemoveAllegianceOfficer => {
                GroupAction::RemoveOfficer(r.client_string16(max_string)?)
            }
            Op::ClearAllegianceOfficers => GroupAction::ClearOfficers,
            Op::ListAllegianceOfficerTitles => GroupAction::ListTitles,
            Op::SetAllegianceOfficerTitle => GroupAction::SetTitle {
                level: r.u32()?,
                title: r.client_string16(max_string)?,
            },
            Op::ClearAllegianceOfficerTitles => GroupAction::ClearTitles,
            Op::DoAllegianceLockAction => GroupAction::Lock(r.u32()?),
            Op::SetAllegianceApprovedVassal => GroupAction::Approve(r.client_string16(max_string)?),
            Op::AllegianceChatBoot => GroupAction::ChatBoot {
                name: r.client_string16(max_string)?,
                reason: r.client_string16(max_string)?,
            },
            Op::AllegianceChatGag => GroupAction::ChatGag {
                name: r.client_string16(max_string)?,
                enabled: r.u32()? != 0,
            },
            Op::ListAllegianceBans => GroupAction::ListBans,
            Op::AddAllegianceBan => GroupAction::AddBan(r.client_string16(max_string)?),
            Op::RemoveAllegianceBan => GroupAction::RemoveBan(r.client_string16(max_string)?),
            Op::BreakAllegianceBoot => GroupAction::Boot {
                name: r.client_string16(max_string)?,
                account: r.u32()? != 0,
            },
            Op::DoAllegianceHouseAction => GroupAction::House(r.u32()?),
            Op::RecallAllegianceHometown => GroupAction::Recall,
            Op::ConfirmationResponse => GroupAction::Confirm {
                kind: r.u32()?,
                token: r.u32()?,
                accepted: r.u32()? != 0,
            },
            _ => return Err(WireError::UnexpectedOpcode(op.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: r.remaining(),
        })
    }
}
