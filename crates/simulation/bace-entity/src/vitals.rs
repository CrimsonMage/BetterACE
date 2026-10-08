//! Authoritative resource proposals; health remains the combatant's sole value.
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntityVital {
    Health,
    Stamina,
    Mana,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VitalPool {
    pub current: u32,
    pub maximum: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VitalMutation {
    pub actor: EntityId,
    pub vital: EntityVital,
    pub before: u32,
    pub after: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VitalMutationResult {
    pub mutation: VitalMutation,
    pub revision: u64,
}
