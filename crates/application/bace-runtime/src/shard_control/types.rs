//! Explicit clock, retained effects and durable drain contracts.
use bace_auth::StaffPrincipal;
use bace_types::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShardClock {
    pub monotonic_millis: u64,
    pub unix_millis: i64,
    /// Captured by the cold adapter; the control owner never reads a wall clock.
    pub local_offset_seconds: i32,
}
#[derive(Clone, Copy, Debug)]
pub enum ShardIssuer<'a> {
    /// Only the separately authenticated host operator path may construct this.
    HostConsole,
    /// Fetch the principal from the current session at command execution.
    Player {
        actor: EntityId,
        name: &'a str,
        principal: StaffPrincipal,
    },
}
impl ShardIssuer<'_> {
    pub(super) fn name(self) -> String {
        match self {
            Self::HostConsole => "CONSOLE".into(),
            Self::Player { name, .. } => name.into(),
        }
    }
    pub(super) fn actor(self) -> Option<EntityId> {
        match self {
            Self::HostConsole => None,
            Self::Player { actor, .. } => Some(actor),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShardChat {
    Broadcast,
    WorldBroadcast,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShardEffect {
    Reply {
        recipient: Option<EntityId>,
        chat: ShardChat,
        text: String,
    },
    Broadcast {
        text: String,
    },
    Audit {
        actor: Option<EntityId>,
        text: String,
    },
    Log {
        text: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardEvent {
    pub sequence: u64,
    pub effect: ShardEffect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShardDrainKind {
    /// Source World.Close only boots access levels below Advocate.
    BootOrdinaryPlayers,
    ShutdownPlayers,
    DisconnectSessions,
    UnloadRegions,
    StopWorld,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShardDrainRequest {
    pub id: u64,
    pub kind: ShardDrainKind,
}
/// Trusted lifecycle receipt. Counts must be obtained from the real owners after
/// draining; queue admission, a timeout or requesting a save is not completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShardDrainReceipt {
    pub request: ShardDrainRequest,
    pub remaining: usize,
    pub unsaved: usize,
    pub pending_operations: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShardError {
    Capacity,
    NotAuthorized,
    InvalidInput,
    Clock,
    StaleReceipt,
    DurabilityPending,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShardShutdown {
    Idle,
    Countdown { deadline_millis: u64 },
    Draining(ShardDrainKind),
    Complete,
}

impl ShardDrainKind {
    /// Official PlayerManager.BootAllPlayers uses account session access, not
    /// character flags or the acting administrator's sudo privilege.
    pub fn includes_player(self, access: bace_auth::AccessLevel) -> bool {
        match self {
            Self::BootOrdinaryPlayers => (access as u8) < bace_auth::AccessLevel::Advocate as u8,
            Self::ShutdownPlayers => true,
            _ => false,
        }
    }
    pub fn boot_message(self) -> Option<(&'static str, &'static str)> {
        (self == Self::BootOrdinaryPlayers).then_some((
            " because the world is now closed",
            "The world is now closed",
        ))
    }
}
