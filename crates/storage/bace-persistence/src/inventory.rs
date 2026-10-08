use crate::{CharacterLease, SaveSnapshot};

/// Relational identity of an item's immediate container and logical slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemLocation {
    pub container: u32,
    pub slot: u32,
}

/// Compare-and-swap ownership change, committed with all affected aggregates.
/// None represents an uncontained world object, not destruction of its snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemTransfer {
    pub item: u32,
    pub expected: Option<ItemLocation>,
    pub destination: Option<ItemLocation>,
}

/// Participants remain reserved until commit is confirmed or rollback resolved.
/// Every affected container ancestor and its character lease must participate.
#[derive(Clone, Debug)]
pub struct InventoryOperation {
    pub operation_id: String,
    pub snapshots: Vec<SaveSnapshot>,
    pub leases: Vec<CharacterLease>,
    pub transfers: Vec<ItemTransfer>,
}
