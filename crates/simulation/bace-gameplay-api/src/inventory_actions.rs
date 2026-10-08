use crate::{ActionContext, ActionResult};
use bace_types::EntityId;
/// Client proposals; simulation supplies ownership, views, geometry and IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryRequest {
    Move {
        item: EntityId,
        container: EntityId,
        placement: i32,
    },
    Drop {
        item: EntityId,
    },
    Equip {
        item: EntityId,
        location: u32,
    },
    SplitToContainer {
        item: EntityId,
        container: EntityId,
        placement: i32,
        amount: i32,
    },
    SplitToWorld {
        item: EntityId,
        amount: i32,
    },
    SplitToWield {
        item: EntityId,
        location: u32,
        amount: i32,
    },
    Merge {
        source: EntityId,
        target: EntityId,
        amount: i32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryRejection {
    NotBound,
    OwnershipMismatch,
    StaleSequence,
    MissingItem,
    MissingContainer,
    Busy,
    OutOfRange,
    Obstructed,
    MissingGeometry,
    InvalidCount,
    InvalidPlacement,
    Attuned,
    TradeReserved,
    ActivePet,
    Capacity,
    Burden,
    AccessDenied,
    StaleView,
    InvalidEquip,
    Requirements,
    UniqueItem,
    Quest,
    InvalidState,
    Overflow,
    DurabilityPending,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryChange {
    pub operation: u64,
    pub objects: Vec<EntityId>,
}
pub type InventoryOutcome = ActionResult<InventoryChange, InventoryRejection>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryCorrelation {
    pub context: ActionContext,
    pub operation: u64,
}
