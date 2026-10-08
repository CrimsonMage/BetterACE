//! Bounded immutable PVS evidence emitted by the authoritative simulation owner.
use crate::CharacterBinding;
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisibilityCandidate {
    pub entity: EntityId,
    /// Pinned ACE Position.Distance2DSquared, computed from accepted positions.
    pub distance_squared: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct VisibilitySnapshot {
    pub binding: CharacterBinding,
    pub observer_epoch: u16,
    pub tick: u64,
    pub candidates: Vec<VisibilityCandidate>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityRejection {
    NotBound,
    MissingActor,
    MissingGeometry,
    Capacity,
    InvalidState,
}
#[derive(Debug)]
pub struct VisibilityRequest {
    pub correlation: u64,
    pub binding: CharacterBinding,
    pub limit: usize,
    pub candidates: Vec<VisibilityCandidate>,
}
impl VisibilityRequest {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && (1..=65536).contains(&self.limit)
            && self.candidates.len() <= self.limit
            && self.candidates.capacity() <= 65536
    }
}
#[derive(Debug)]
pub struct VisibilityOutcome {
    pub correlation: u64,
    pub result: Result<VisibilitySnapshot, (VisibilityRejection, Vec<VisibilityCandidate>)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcceptedMoveParameters {
    pub flags: u32,
    pub distance_to_object: f32,
    pub min_distance: f32,
    pub fail_distance: f32,
    pub speed: f32,
    pub walk_run_threshold: f32,
    /// Legacy motion-network headings are degrees.
    pub desired_heading: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcceptedTurnParameters {
    pub flags: u32,
    pub speed: f32,
    pub desired_heading: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ServerMovementGoal {
    MoveToObject {
        target: EntityId,
        parameters: AcceptedMoveParameters,
        run_rate: f32,
    },
    MoveToPosition {
        cell: u32,
        position: [f32; 3],
        parameters: AcceptedMoveParameters,
        run_rate: f32,
    },
    TurnToObject {
        target: EntityId,
        parameters: AcceptedTurnParameters,
    },
    TurnToHeading {
        parameters: AcceptedTurnParameters,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcceptedMovementGoal {
    pub control_owner: u64,
    pub control_sequence: u64,
    pub goal: ServerMovementGoal,
    /// Present only for MoveToObject; read from the current accepted target owner.
    pub target_position: Option<(u32, [f32; 3])>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AcceptedMotionDomain {
    Casting,
    Physical,
    Interaction,
    Inventory,
    Crafting,
    Recall,
    Death,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcceptedMotionAction {
    pub domain: AcceptedMotionDomain,
    pub owner: u64,
    pub sequence: u64,
    pub motion: u32,
    pub speed: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AcceptedObjectMotion {
    pub autonomous: bool,
    pub style: u32,
    pub forward_motion: u32,
    pub forward_rate: f32,
    pub sidestep_rate: f32,
    pub turn_rate: f32,
    pub actions: Vec<AcceptedMotionAction>,
    pub goal: Option<AcceptedMovementGoal>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AcceptedObjectView {
    pub entity: EntityId,
    pub cell: u32,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub heading_radians: f32,
    pub grounded: bool,
    pub epoch: u16,
    pub held: bool,
    pub motion: Result<Option<AcceptedObjectMotion>, ObjectViewRejection>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectViewRejection {
    NotBound,
    MissingActor,
    MissingMotion,
    Capacity,
    InvalidState,
    InTransit,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectViewSnapshot {
    pub binding: CharacterBinding,
    pub observer_epoch: u16,
    pub tick: u64,
    pub views: Vec<(EntityId, Result<AcceptedObjectView, ObjectViewRejection>)>,
}
#[derive(Debug)]
pub struct ObjectViewRequest {
    pub correlation: u64,
    pub binding: CharacterBinding,
    pub entities: Vec<EntityId>,
}
impl ObjectViewRequest {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && self.entities.len() <= 4097
            && self.entities.iter().all(|id| id.0 != 0)
            && self.entities.windows(2).all(|p| p[0] < p[1])
    }
}
#[derive(Debug)]
pub struct ObjectViewOutcome {
    pub correlation: u64,
    pub result: Result<ObjectViewSnapshot, ObjectViewRejection>,
}

/// Source PVS audience captured on the same owner turn as projectile insertion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileLaunchObserver {
    pub binding: CharacterBinding,
    pub epoch: u16,
    pub distance_squared: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AcceptedProjectileLaunch {
    pub tick: u64,
    pub view: AcceptedObjectView,
    pub observers: Vec<ProjectileLaunchObserver>,
}

/// The same accepted pose and audience proof applies to actor/item births.
pub type AcceptedObjectBirth = AcceptedProjectileLaunch;
