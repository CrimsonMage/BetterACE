//! Accepted durable item-XP publication. The item identity selects its sequence
//! owner even though ACE's private Int64 update does not serialize that identity.
use bace_types::EntityId;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemExperienceEvent {
    pub actor: EntityId,
    pub item: EntityId,
    pub total: u64,
    pub level_up: Option<(String, u32)>,
}
