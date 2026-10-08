/// Mutable rare state belongs to the character owner. This is a domain contract,
/// not a persisted gameplay struct; the storage codec has a separate frozen DTO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharacterRareState {
    pub character: u32,
    pub random_identity: [u8; 16],
    pub key_version: u32,
    pub attempt_ordinal: u64,
    pub timer_ordinal: u64,
    pub next_realtime_at: Option<u64>,
    pub last_effective_time: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RareKillContext {
    pub character: u32,
    pub player_level: u32,
    pub creature_level: u32,
    pub lootable_monster_death: bool,
    pub now_unix_seconds: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RareDecision {
    pub character: u32,
    pub eligible: bool,
    pub previous: CharacterRareState,
    pub next: CharacterRareState,
    pub profile_id: u32,
    pub standard_success: bool,
    pub realtime_success: bool,
    pub award: Option<RareAward>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RareAward {
    pub tier: u8,
    pub template: u32,
}
