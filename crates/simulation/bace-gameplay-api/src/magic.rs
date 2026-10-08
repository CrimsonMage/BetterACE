use crate::ActionResult;
use bace_types::EntityId;

/// Client spell IDs/targets remain proposals; caster identity is session-bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastRequest {
    Targeted { target: EntityId, spell: u32 },
    Untargeted { spell: u32 },
    Cancel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastRejection {
    NotBound,
    OwnershipMismatch,
    MissingActor,
    StaleSequence,
    UnknownSpell,
    UnlearnedSpell,
    UntrainedSchool,
    InvalidTarget,
    OutOfRange,
    Obstructed,
    Busy,
    SchoolRecovery,
    StreakCooldown,
    Resisted,
    Dead,
    PortalSpace,
    WrongMode,
    MissingComponents,
    InsufficientMana,
    Fizzled,
    MissingAssets,
    Capacity,
    InvalidState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastChange {
    Started { cast: u64 },
    Completed { cast: u64 },
    Cancelled { cast: u64 },
}
pub type CastOutcome = ActionResult<CastChange, CastRejection>;
