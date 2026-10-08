use crate::ActionResult;
use bace_types::EntityId;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HousingRequest {
    Buy {
        slumlord: EntityId,
        payments: Vec<EntityId>,
    },
    Rent {
        slumlord: EntityId,
        payments: Vec<EntityId>,
    },
    Abandon,
    Query,
    SetOpen(bool),
    SetStorageOpen(bool),
    SetHooksVisible(bool),
    SetGuest {
        guest: EntityId,
        storage: bool,
    },
    RemoveGuest(EntityId),
    ClearGuests,
    ClearStorageGuests,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HousingRejection {
    NotBound,
    OwnershipMismatch,
    StaleSequence,
    MissingHouse,
    NotOwner,
    AlreadyOwned,
    Requirements,
    OutOfRange,
    AccessDenied,
    InvalidPayment,
    InsufficientPayment,
    StaleView,
    Capacity,
    Overflow,
    DurabilityPending,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HousingChange {
    Changed {
        house: EntityId,
        generation: u64,
        operation: u64,
    },
    Unchanged,
}
pub type HousingOutcome = ActionResult<HousingChange, HousingRejection>;
