use crate::PlacementOperation;

/// Exact vendor marker CAS, separate from world item snapshots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorStockWrite {
    pub marker_object_id: u32,
    pub expected_version: i64,
    pub expected_stock_revision: u64,
    pub mutation_revision: u64,
    pub bytes: Vec<u8>,
}

/// The stock marker and all item/player placement changes commit together under
/// one world execution epoch and operation ID. Keep this exact request on retry.
#[derive(Clone, Debug)]
pub struct VendorStockOperation {
    pub world_epoch: u64,
    /// Committed vendor aggregate version observed by the simulation proposal.
    pub vendor_expected_version: i64,
    pub inventory: PlacementOperation,
    pub marker: VendorStockWrite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredVendorStock {
    pub vendor_object_id: u32,
    pub marker_object_id: u32,
    pub persisted_version: i64,
    pub bytes: Vec<u8>,
}

/// One SQL view of the durable vendor aggregate and its world placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredVendorSource {
    pub aggregate: crate::StoredAggregate,
    pub cell: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredVendorStockItem {
    pub aggregate: crate::StoredAggregate,
    pub placement: crate::DurableItemPlace,
}

/// Marker followed by complete roots/descendants in frozen stock order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredVendorStockForest {
    pub marker: StoredVendorStock,
    pub items: Vec<StoredVendorStockItem>,
}

/// One hierarchy-sequenced view of the durable world vendor and its optional
/// loaded stock forest. A purchase cannot commit between these two reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredVendorState {
    pub source: StoredVendorSource,
    pub forest: Option<StoredVendorStockForest>,
}
