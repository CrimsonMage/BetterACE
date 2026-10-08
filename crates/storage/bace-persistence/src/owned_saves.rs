use crate::{CharacterLease, ItemLocation, SaveSnapshot, StoredAggregate};
/// Routine mutation of existing aggregates. Ownership relationships cannot change.
/// Participants include changed objects and every container/house-owner ancestor;
/// lock-only ancestors need no redundant save snapshot. Leases fence all characters.
#[derive(Clone, Debug)]
pub struct OwnedSaveBatch {
    pub snapshots: Vec<SaveSnapshot>,
    pub participants: Vec<u32>,
    pub leases: Vec<CharacterLease>,
}
#[derive(Clone, Copy, Debug)]
pub struct InventoryLoadLimits {
    pub max_items: usize,
    pub max_depth: usize,
    pub max_total_bytes: usize,
}
impl Default for InventoryLoadLimits {
    fn default() -> Self {
        Self {
            max_items: 1024,
            max_depth: 16,
            max_total_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Debug)]
pub struct StoredInventoryItem {
    pub aggregate: StoredAggregate,
    pub location: ItemLocation,
    pub depth: u16,
    pub pack_slot: bool,
    pub equipped: u32,
}
#[derive(Clone, Debug)]
pub struct LoadedInventory {
    pub lease: CharacterLease,
    pub items: Vec<StoredInventoryItem>,
}
