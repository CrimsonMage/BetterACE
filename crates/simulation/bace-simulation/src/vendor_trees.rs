//! Immutable stock construction graph. Roots belong to the vendor stock owner;
//! descendants always have explicit container identities and placement slots.
use bace_types::EntityId;
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone)]
pub struct PreparedVendorTree {
    pub root: EntityId,
    pub template: Arc<bace_content::WeenieV1>,
    pub items: Vec<bace_inventory::InventoryItem>,
    pub containers: Vec<bace_inventory::InventoryContainer>,
    pub templates: BTreeMap<EntityId, Arc<bace_content::WeenieV1>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct VendorContents {
    pub items: Vec<bace_inventory::InventoryItem>,
    pub containers: Vec<bace_inventory::InventoryContainer>,
}
/// Exact source-ordered first-Use Shop entry. The inventory/source snapshot
/// adapter uses destination flags beside the simulation-owned stock tree.
#[derive(Clone)]
pub struct PreparedVendorLazyItem {
    pub tree: PreparedVendorTree,
    pub display_quantity: i32,
    pub source_destinations: BTreeMap<EntityId, Option<u8>>,
}
#[derive(Clone)]
pub struct PreparedVendorLazyStock {
    pub vendor: EntityId,
    /// Exact existing durable vendor snapshot version. Stock children require
    /// that parent identity in the same SQL ownership transaction.
    pub vendor_expected_version: i64,
    pub marker: EntityId,
    pub operation_id: String,
    pub source_revision: u64,
    pub source_hash: [u8; 32],
    pub expected_stock_revision: u64,
    pub entries: Vec<PreparedVendorLazyItem>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorLazyStockTicket {
    pub vendor: EntityId,
    pub vendor_expected_version: i64,
    pub marker: EntityId,
    pub operation_id: String,
    pub source_revision: u64,
    pub source_hash: [u8; 32],
    pub expected_stock_revision: u64,
    pub stock_revision: u64,
    pub item_ids: Vec<EntityId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorLazyStockReceipt {
    pub vendor: EntityId,
    pub marker: EntityId,
    pub operation_id: String,
    pub marker_version: i64,
    pub items: Vec<(EntityId, i64)>,
}
impl PreparedVendorTree {
    pub fn valid_bounds(&self) -> bool {
        self.root.0 != 0
            && self.items.len() <= 1023
            && self.containers.len() <= 1024
            && self.templates.len() == self.items.len()
    }
}
