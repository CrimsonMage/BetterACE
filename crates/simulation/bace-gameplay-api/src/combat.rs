use crate::ActionResult;
use bace_types::EntityId;

/// Decoded client intent. The simulation must recheck the authenticated binding,
/// sequence, actor state and authoritative geometry before applying it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatRequest {
    TargetedMelee {
        target: EntityId,
        height: u32,
        power: f32,
    },
    TargetedMissile {
        target: EntityId,
        height: u32,
        accuracy: f32,
    },
    ChangeMode(u32),
    CancelAttack,
    QueryHealth(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatRejection {
    NotBound,
    OwnershipMismatch,
    MissingActor,
    StaleSequence,
    InvalidRequest,
    UnsupportedMode,
    MissingCombatProfile,
    Dead,
    Busy,
    OutOfRange,
    Obstructed,
    Capacity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatChange {
    Mode(u32),
    AttackStarted {
        target: EntityId,
    },
    Cancelled,
    Health {
        target: EntityId,
        current: u32,
        maximum: u32,
    },
}
pub type CombatOutcome = ActionResult<CombatChange, CombatRejection>;
