use crate::ActionResult;
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UseDoor {
    pub door: EntityId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorChange {
    Motion {
        door: EntityId,
        open: bool,
        generation: u64,
        server_control: u16,
        animation_sequence: u16,
    },
    Busy,
    Locked,
    Unchanged,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorRejection {
    NotBound,
    OwnershipMismatch,
    MissingActor,
    StaleSequence,
    MissingDoor,
    OutOfRange,
    Obstructed,
    MissingGeometry,
    Capacity,
    InvalidState,
}
pub type DoorOutcome = ActionResult<DoorChange, DoorRejection>;
