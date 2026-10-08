use crate::{CharacterLease, SaveSnapshot};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurableItemPlace {
    Contained {
        container: u32,
        slot: u32,
        pack_slot: bool,
        equipped: u32,
    },
    World {
        cell: u32,
    },
    Removed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacementChange {
    pub item: u32,
    pub expected: Option<DurableItemPlace>,
    pub destination: DurableItemPlace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageViewFence {
    pub actor: u32,
    pub house: u32,
    pub generation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementOperation {
    pub operation_id: String,
    pub snapshots: Vec<SaveSnapshot>,
    pub participants: Vec<u32>,
    pub leases: Vec<CharacterLease>,
    pub changes: Vec<PlacementChange>,
    pub storage_views: Vec<StorageViewFence>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HouseOwnershipChange {
    pub house: u32,
    pub house_id: u32,
    pub expected_owner: Option<u32>,
    pub expected_generation: u64,
    pub owner: Option<u32>,
    pub generation: u64,
}
#[derive(Clone, Debug)]
pub struct HousingOperation {
    pub inventory: PlacementOperation,
    pub ownership: HouseOwnershipChange,
}
#[derive(Clone, Debug)]
pub struct LocatedSnapshot {
    pub aggregate: crate::StoredAggregate,
    pub placement: DurableItemPlace,
    pub depth: u16,
}
#[derive(Clone, Debug)]
pub struct HouseMaintenanceSnapshot {
    pub aggregate: crate::StoredAggregate,
    pub owner: Option<CharacterLease>,
}

/// A valuable world-owned placement, fenced by the durable execution epoch.
/// Resolve the exact old request after uncertainty; never rebind its epoch.
#[derive(Clone, Debug)]
pub struct WorldPlacementOperation {
    pub world_epoch: u64,
    pub inventory: PlacementOperation,
}
