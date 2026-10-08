//! Exact save-before-eviction snapshots. This is an in-memory handoff, never a save DTO.
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq)]
pub struct RegionUnloadItem {
    pub item: bace_inventory::InventoryItem,
    pub transient: bool,
    pub container: Option<bace_inventory::InventoryContainer>,
    pub registry_revision: Option<u64>,
    pub enchantments: Vec<bace_magic::EnchantmentEntry>,
    pub position: Option<bace_interactions::PortalPosition>,
    pub corpse: Option<bace_world::CorpseState>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RegionUnloadTicket {
    pub operation: u64,
    pub landblock: u16,
    pub epoch: u64,
    pub items: Vec<RegionUnloadItem>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionUnloadReceipt {
    pub operation: u64,
    pub landblock: u16,
    pub epoch: u64,
    pub revisions: Vec<(EntityId, u64, Option<u64>)>,
}
pub(crate) struct PendingRegionUnload {
    pub epoch: u64,
    pub notified: BTreeSet<EntityId>,
    pub ticket: Option<RegionUnloadTicket>,
    pub submitted: bool,
    pub saved: bool,
    pub blocked: Option<crate::ResidencyError>,
}
#[derive(Default)]
pub(crate) struct RegionUnloads {
    pub pending: BTreeMap<u16, PendingRegionUnload>,
    pub roots: BTreeMap<u16, Vec<EntityId>>,
    pub next: u64,
    pub cursor: Option<u16>,
}
