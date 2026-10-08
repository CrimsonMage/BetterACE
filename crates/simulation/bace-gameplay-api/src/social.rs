//! Authenticated social requests and accepted projections. No wire or persistence ownership.
use crate::ActionContext;
use bace_types::{AccountId, EntityId};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChatChannel {
    Local,
    Tell,
    Emote,
    SoulEmote,
    Fellowship,
    Allegiance,
    General,
    Trade,
    Lfg,
    Roleplay,
    Society(u32),
    Olthoi,
    Audit,
    Legacy(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedChat {
    pub sequence: u64,
    pub unix_seconds: i64,
    pub sender: EntityId,
    pub sender_name: String,
    pub channel: ChatChannel,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialRequest {
    Talk(String),
    Tell {
        text: String,
        target_name: String,
    },
    TalkDirect {
        text: String,
        target: EntityId,
    },
    Emote(String),
    SoulEmote(String),
    Channel {
        channel: u32,
        text: String,
    },
    Turbine {
        context_id: u32,
        dispatch: u32,
        channel: u32,
        chat_type: u32,
        text: String,
    },
    SetAfk(bool),
    SetAfkMessage(String),
    AddFriend(String),
    RemoveFriend(EntityId),
    RemoveAllFriends,
    AddChannel(u32),
    RemoveChannel(u32),
    CharacterSquelch {
        enabled: bool,
        target: EntityId,
        name: String,
        message_type: u32,
    },
    AccountSquelch {
        enabled: bool,
        name: String,
    },
    GlobalSquelch {
        enabled: bool,
        message_type: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FellowshipRequest {
    Create { name: String, share_xp: bool },
    Recruit(EntityId),
    Quit { disband: bool },
    Dismiss(EntityId),
    AssignLeader(EntityId),
    ChangeOpenness(bool),
    Panel(bool),
    Confirm { token: u32, accepted: bool },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AllegianceRequest {
    Swear(EntityId),
    Break(EntityId),
    Update { enabled: bool },
    Info(String),
    QueryMotd,
    SetMotd(String),
    ClearMotd,
    QueryName,
    SetName(String),
    ClearName,
    ListOfficers,
    SetOfficer { name: String, level: u32 },
    RemoveOfficer(String),
    ClearOfficers,
    ListOfficerTitles,
    SetOfficerTitle { level: u32, title: String },
    ClearOfficerTitles,
    Lock(u32),
    ApproveVassal(String),
    ChatBoot { name: String, reason: String },
    ChatGag { name: String, enabled: bool },
    ListBans,
    AddBan(String),
    RemoveBan(String),
    Boot { name: String, account: bool },
    HouseAction(u32),
    RecallHometown,
    Confirm { token: u32, accepted: bool },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocialError {
    Invalid,
    Missing,
    Offline,
    Forbidden,
    Gagged,
    Squelched,
    Duplicate,
    Capacity,
    Busy,
    Stale,
    Overflow,
    NotMember,
    NotLeader,
    Locked,
    Full,
    Declined,
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocialOutcome {
    pub context: ActionContext,
    /// True only when the owner declined before consuming the action sequence.
    pub retryable: bool,
    pub result: Result<(), SocialError>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllegianceRelation {
    pub character: EntityId,
    pub monarch: EntityId,
    pub patron: Option<EntityId>,
    pub rank: u32,
    pub officer_level: u32,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocialIdentity {
    pub character: EntityId,
    pub account: AccountId,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocialFriend {
    pub character: EntityId,
    pub name: String,
    pub online: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocialSquelch {
    pub character: EntityId,
    pub account: Option<AccountId>,
    pub name: String,
    pub mask: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SocialEvent {
    EquipmentMana {
        recipient: EntityId,
        name: String,
        depleted: bool,
    },
    Allegiance {
        recipient: EntityId,
        info_response: bool,
        profile: AllegianceProfileSnapshot,
    },
    Fellowship {
        recipients: Vec<EntityId>,
        snapshot: FellowshipSnapshot,
    },
    FellowshipLeft {
        recipients: Vec<EntityId>,
        actor: EntityId,
        dismissed: bool,
        disbanded: bool,
    },
    Chat {
        accepted: AcceptedChat,
        wire: ChatDelivery,
        recipients: Vec<EntityId>,
    },
    TurbineEcho {
        recipient: EntityId,
        sender_name: String,
        channel: u32,
        chat_type: u32,
        text: String,
    },
    Transient {
        recipient: EntityId,
        text: String,
    },
    System {
        recipient: EntityId,
        text: String,
        chat_type: u32,
    },
    Error {
        recipient: EntityId,
        code: u32,
        argument: Option<String>,
    },
    Friends {
        recipient: EntityId,
        kind: u32,
        entries: Vec<SocialFriend>,
    },
    Squelches {
        recipient: EntityId,
        entries: Vec<SocialSquelch>,
        global_mask: u32,
    },
    TurbineResponse {
        recipient: EntityId,
        context_id: u32,
        dispatch: u32,
        result: i32,
        chat_type: u32,
    },
    Channels {
        recipient: EntityId,
        allegiance: u32,
        society: u32,
    },
    Confirmation {
        recipient: EntityId,
        kind: u32,
        token: u32,
        text: String,
    },
    Afk {
        recipient: EntityId,
        enabled: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardXpKind {
    Kill,
    Quest,
    Fellowship,
    Allegiance,
    Admin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RewardSharing {
    pub fellowship: bool,
    pub allegiance: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EarnedExperience {
    pub source: EntityId,
    pub amount: u64,
    pub kind: RewardXpKind,
    pub sharing: RewardSharing,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllegianceSanctuary {
    pub cell: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FellowSnapshot {
    pub actor: EntityId,
    pub name: String,
    pub level: u32,
    pub maximum: [u32; 3],
    pub current: [u32; 3],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FellowshipSnapshot {
    pub id: u64,
    pub revision: u64,
    pub name: String,
    pub leader: EntityId,
    pub members: Vec<FellowSnapshot>,
    pub share_xp: bool,
    pub even_share: bool,
    pub open: bool,
    pub locked: bool,
    pub departed: Vec<(EntityId, u64)>,
    pub locks: Vec<(String, u64, u32)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllegianceNodeSnapshot {
    pub actor: EntityId,
    pub cached: u32,
    pub tithed: u32,
    pub online: bool,
    pub may_pass_up: bool,
    pub gender: u8,
    pub heritage: u8,
    pub rank: u16,
    pub level: u32,
    pub loyalty: u16,
    pub leadership: u16,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AllegianceProfileSnapshot {
    pub subject: EntityId,
    pub rank: u32,
    pub total_members: u32,
    pub total_vassals: u32,
    pub chat_room: u32,
    pub name: String,
    pub sanctuary: Option<AllegianceSanctuary>,
    pub monarch: Option<AllegianceNodeSnapshot>,
    pub records: Vec<(EntityId, AllegianceNodeSnapshot)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatDelivery {
    Speech,
    Emote,
    SoulEmote,
    Tell,
    LegacyChannel(u32),
    Turbine { channel: u32, chat_type: u32 },
}
