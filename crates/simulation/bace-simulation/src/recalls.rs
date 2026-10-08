//! Command recall state belongs to the simulation owner; destinations are
//! immutable accepted content, while house/allegiance permission reads stay live.
use bace_gameplay_api::ActionContext;
use bace_interactions::{PortalPosition, RecallError, RecallKind};
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedRecallLocations {
    pub marketplace: PortalPosition,
    pub pk_arena: [PortalPosition; 5],
    pub pkl_arena: [PortalPosition; 5],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedRecallHouse {
    pub house: EntityId,
    pub slumlord: EntityId,
    /// ACE HouseType: Villa=2, Mansion=3.
    pub house_type: u32,
    pub destination: PortalPosition,
}
#[derive(Clone, Debug, PartialEq)]
pub enum RecallCommand {
    /// Live adapter input uses the captured revision and a verified DAT chain.
    StartPrepared {
        context: ActionContext,
        kind: RecallKind,
        before_revision: u64,
        animation_seconds: f64,
        style: Option<Arc<bace_motion::PreparedMotionChain>>,
        motion: Arc<bace_motion::PreparedMotionChain>,
    },
    /// Timer-only domain entry retained for synthetic owner qualification.
    Start {
        context: ActionContext,
        kind: RecallKind,
        animation_seconds: f64,
    },
    UseBinding {
        context: ActionContext,
        object: EntityId,
        before_revision: u64,
        animation_seconds: f64,
        style: Option<Arc<bace_motion::PreparedMotionChain>>,
        motion: Arc<bace_motion::PreparedMotionChain>,
    },
    Cancel {
        actor: EntityId,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum RecallEvent {
    /// Cold evidence changed before authorization. The exact client action may
    /// be recaptured and retried; its sequence has not been consumed.
    Retry {
        context: ActionContext,
    },
    BindingStarted {
        context: ActionContext,
        object: EntityId,
        until_tick: u64,
    },
    BindingActionStarted {
        context: ActionContext,
        object: EntityId,
    },
    BindingStaged {
        context: ActionContext,
        object: EntityId,
        operation: u64,
        allegiance: bool,
        use_message: Arc<str>,
        stamina_after: Option<u32>,
    },
    Started {
        context: ActionContext,
        kind: RecallKind,
        motion: u32,
        until_tick: u64,
        mana_after: Option<u32>,
        combat_mode_changed: bool,
    },
    Rejected {
        context: ActionContext,
        error: RecallError,
    },
    Staged {
        context: ActionContext,
        kind: RecallKind,
        operation: u64,
    },
    Cancelled {
        context: ActionContext,
    },
}
pub(crate) struct PendingRecall {
    pub context: ActionContext,
    pub kind: RecallKind,
    pub start: PortalPosition,
    pub start_epoch: u16,
    pub due: u64,
    pub destination: PortalPosition,
    pub motion: Option<RecallMotion>,
}
pub(crate) struct RecallMotion {
    pub token: bace_motion::MotionToken,
    pub completed: Option<bool>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedBindingObject {
    pub entity: EntityId,
    pub kind: bace_interactions::BindingKind,
    pub use_radius: f32,
    pub use_message: Arc<str>,
}
pub(crate) struct PendingBinding {
    pub context: ActionContext,
    pub object: EntityId,
    pub due: u64,
    pub motion_owner: u64,
    pub start_epoch: u16,
    pub completed: Option<bool>,
    pub action_sequence: u64,
    pub action_started: bool,
    pub action_announced: bool,
}
pub(crate) struct Recalls {
    pub bindings: BTreeMap<EntityId, PreparedBindingObject>,
    pub pending_bindings: BTreeMap<EntityId, PendingBinding>,
    pub pending: BTreeMap<EntityId, PendingRecall>,
    pub houses: BTreeMap<EntityId, PreparedRecallHouse>,
    pub locations: Option<Arc<PreparedRecallLocations>>,
    pub random: Option<Arc<bace_random::RandomRoot>>,
    pub epoch: u64,
    pub next: u64,
    pub events: VecDeque<RecallEvent>,
    pub scratch: Vec<EntityId>,
    pub capacity: usize,
}
impl Recalls {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.min(4096);
        Self {
            bindings: BTreeMap::new(),
            pending_bindings: BTreeMap::new(),
            pending: BTreeMap::new(),
            houses: BTreeMap::new(),
            locations: None,
            random: None,
            epoch: 0,
            next: 0,
            events: VecDeque::with_capacity(capacity),
            scratch: Vec::with_capacity(capacity),
            capacity,
        }
    }
    pub fn has_state(&self) -> bool {
        !self.pending.is_empty() || !self.pending_bindings.is_empty() || !self.events.is_empty()
    }
}

pub(crate) type PreparedRecallChains = (
    Option<Arc<bace_motion::PreparedMotionChain>>,
    Arc<bace_motion::PreparedMotionChain>,
);
