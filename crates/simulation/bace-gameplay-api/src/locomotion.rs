//! Authenticated movement controls. Client physical observations never enter the
//! command as an accepted pose, velocity, contact or authoritative clock.
use crate::ActionContext;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawLocomotionState {
    pub style: u32,
    pub current_hold: u32,
    pub forward: (u32, u32, f32),
    pub sidestep: (u32, u32, f32),
    pub turn: (u32, u32, f32),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LocomotionRequest {
    State(RawLocomotionState),
    Jump { extent: f32 },
    ObservePosition,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocomotionCommand {
    pub correlation: u64,
    pub context: ActionContext,
    pub teleport: u16,
    pub request: LocomotionRequest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocomotionRejection {
    NotBound,
    Stale,
    Busy,
    Invalid,
    MissingAssets,
    TooTired,
    Capacity,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LocomotionOutcome {
    pub correlation: u64,
    pub context: ActionContext,
    pub tick: u64,
    pub result: Result<LocomotionAccepted, LocomotionRejection>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LocomotionAccepted {
    pub view: crate::visibility::AcceptedObjectView,
    pub stamina: Option<(u32, u32)>,
    pub vital_revision: u64,
}
impl LocomotionCommand {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && self.context.actor.0 != 0
            && match self.request {
                LocomotionRequest::State(raw) => [raw.forward.2, raw.sidestep.2, raw.turn.2]
                    .into_iter()
                    .all(f32::is_finite),
                LocomotionRequest::Jump { extent } => extent.is_finite(),
                LocomotionRequest::ObservePosition => true,
            }
    }
}
