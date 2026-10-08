//! Source corpse deadlines with retained durable tombstone proposals.
use bace_types::EntityId;
use std::collections::{BTreeMap, VecDeque};
/// Final-loot shortening is committed with the inventory transfer, never ahead
/// of its durable receipt. ACE WorldObject_Decay caps empty corpses at 15 seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorpseDecayChange {
    pub corpse: EntityId,
    pub death_operation: u64,
    pub before_expires_tick: u64,
    pub after_expires_tick: u64,
    pub prepared_tick: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CorpseExpiryTicket {
    pub corpse: EntityId,
    pub death_operation: u64,
    pub expires_tick: u64,
    pub inventory: crate::InventoryTicket,
    pub transient: Vec<EntityId>,
    pub spill: Option<CorpseSpillIntent>,
    pub enchantments: BTreeMap<EntityId, Vec<bace_magic::EnchantmentEntry>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CorpseSpillIntent {
    pub roots: Vec<EntityId>,
    pub position: bace_gameplay_api::GeneratorLocation,
}
pub struct PreparedCorpseSpill {
    pub operation: u64,
    pub actors: Vec<bace_entity::Actor>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseExpiryPhase {
    Destroying,
    Removed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorpseExpiryEvent {
    pub corpse: EntityId,
    pub death_operation: u64,
    pub phase: CorpseExpiryPhase,
}
pub(crate) struct PendingCorpseExpiry {
    pub ticket: CorpseExpiryTicket,
    pub submitted: bool,
    pub spill: Option<Vec<bace_entity::Actor>>,
}
#[derive(Clone, Copy)]
pub(crate) struct CorpseDeadline {
    pub operation: u64,
    pub tick: u64,
}
pub(crate) struct CorpseExpiries {
    pub retiring: BTreeMap<EntityId, (u64, u64)>,
    pub deadlines: BTreeMap<EntityId, CorpseDeadline>,
    pub pending: BTreeMap<u64, PendingCorpseExpiry>,
    pub events: VecDeque<CorpseExpiryEvent>,
    pub capacity: usize,
    pub cursor: Option<EntityId>,
    pub blocked: BTreeMap<EntityId, crate::PlayerDeathError>,
}
impl CorpseExpiries {
    pub fn new(capacity: usize) -> Self {
        Self {
            retiring: Default::default(),
            deadlines: Default::default(),
            pending: Default::default(),
            events: VecDeque::with_capacity(capacity.min(4096)),
            capacity: capacity.min(4096),
            cursor: None,
            blocked: Default::default(),
        }
    }
    pub fn owns_inventory(&self, operation: u64) -> bool {
        self.pending.contains_key(&operation)
    }
    pub fn has_state(&self) -> bool {
        !self.deadlines.is_empty()
            || !self.pending.is_empty()
            || !self.events.is_empty()
            || !self.retiring.is_empty()
    }
}
