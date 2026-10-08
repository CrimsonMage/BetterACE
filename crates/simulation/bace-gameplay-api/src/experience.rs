//! Accepted durable character-XP publication; contains no mutable gameplay owner.
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExperienceState {
    pub total: u64,
    pub available: u64,
    pub level: u32,
    pub available_skill_credits: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperienceEvent {
    pub update_properties: bool,
    pub actor: EntityId,
    pub before: ExperienceState,
    pub after: ExperienceState,
    /// Source UpdateXpAndLevel skips XP properties when already at maximum level.
    pub maximum_level: u32,
    /// Source Quest message uses the original award even when maximum-level capped.
    pub quest_amount: Option<u64>,
    /// Required only when a nonmaximum level-up earns no credit; zero means no
    /// future credit exists in the accepted level table, as in the pinned source.
    pub next_credit_level: Option<u32>,
    /// Accepted SetMaxVitals updates, (secondary attribute ID,current value).
    pub vitals: Vec<(u32, u32)>,
}
