use crate::StoredAggregate;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnershipState {
    Offline,
    Loading,
    Online,
    LoggingOut,
}
impl OwnershipState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Loading => "loading",
            Self::Online => "online",
            Self::LoggingOut => "logging_out",
        }
    }
}
/// Epoch identifies ownership, independently of mutation revision and database CAS version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharacterLease {
    pub character_id: u32,
    pub epoch: i64,
    pub state: OwnershipState,
}
#[derive(Debug)]
pub struct CharacterLoad {
    pub lease: CharacterLease,
    pub snapshot: StoredAggregate,
    pub cached_xp: u64,
}
/// Semantic identity survives rebasing against newer aggregate snapshots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfflineXpEvent {
    pub event_id: String,
    pub source_character: u32,
    pub target_character: u32,
    pub amount: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XpReceipt {
    pub event_id: String,
    pub cached_xp: u64,
    pub newly_applied: bool,
}

/// Exact committed login sequence receipt. Source Int32 range is enforced by the
/// database; ownership epochs are deliberately not protocol instance counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnlineLoginReceipt {
    pub lease: CharacterLease,
    pub total_logins: u32,
}
