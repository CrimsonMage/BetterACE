//! Source-prepared healer use held through one item/player durable receipt.
use bace_entity::{EntityVital, VitalMutation};
use bace_gameplay_api::ActionContext;
use bace_inventory::ActivationRequirements;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{InventoryReceipt, InventoryTicket};
use bace_motion::{MotionToken, PreparedMotionChain};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct PreparedHealingKit {
    pub vital: EntityVital,
    pub boost_value: i32,
    pub healkit_mod: f32,
    pub unlimited_use: bool,
    pub max_structure: Option<u32>,
    pub template_value: Option<u32>,
    pub activation: Option<ActivationRequirements>,
}

#[derive(Clone, Debug)]
pub enum HealingKitCommand {
    Use {
        correlation: u64,
        context: ActionContext,
        item: EntityId,
        item_revision: u64,
        target: EntityId,
        profile: PreparedHealingKit,
        motion: Arc<PreparedMotionChain>,
    },
    Commit {
        correlation: u64,
        operation: u64,
        receipt: Option<InventoryReceipt>,
    },
    Reject {
        correlation: u64,
        operation: u64,
    },
}
impl HealingKitCommand {
    pub fn valid_bounds(&self) -> bool {
        match self {
            Self::Use {
                correlation,
                context,
                item,
                item_revision,
                target,
                profile,
                motion,
            } => {
                *correlation != 0
                    && context.actor.0 != 0
                    && item.0 != 0
                    && *item_revision != 0
                    && target.0 != 0
                    && profile.healkit_mod.is_finite()
                    && (0.0..=1000.0).contains(&profile.healkit_mod)
                    && profile.max_structure.is_none_or(|n| n <= u16::MAX as u32)
                    && motion.motion == 0x1000_010e
                    && motion.speed == 1.0
            }
            Self::Commit {
                correlation,
                operation,
                receipt,
            } => {
                *correlation != 0
                    && *operation != 0
                    && receipt
                        .as_ref()
                        .is_none_or(|r| r.operation == *operation && r.revisions.len() <= 1024)
            }
            Self::Reject {
                correlation,
                operation,
            } => *correlation != 0 && *operation != 0,
        }
    }
    pub fn correlation(&self) -> u64 {
        match self {
            Self::Use { correlation, .. }
            | Self::Commit { correlation, .. }
            | Self::Reject { correlation, .. } => *correlation,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HealingKitTicket {
    pub operation: u64,
    pub context: ActionContext,
    pub target: EntityId,
    pub inventory: Option<InventoryTicket>,
    pub actor_revision: u64,
    pub vitals: Vec<VitalMutation>,
    pub success: bool,
    pub critical: bool,
    pub heal_amount: u32,
    pub stamina_cost: u32,
}

#[derive(Clone, Debug)]
pub enum HealingKitDecision {
    MotionStarted {
        actor: EntityId,
        style: u32,
        command: u32,
    },
    Proposed(HealingKitTicket),
    Committed(HealingKitTicket),
    Rejected(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealingKitError {
    Ownership,
    UnsupportedTarget,
    NotTrained,
    FullVital,
    Busy,
    Stale,
    Capacity,
    Content,
    Receipt,
    MotionDisrupted,
}
#[derive(Clone, Debug)]
pub struct HealingKitOutcome {
    pub correlation: u64,
    pub context: Option<ActionContext>,
    pub result: Result<HealingKitDecision, HealingKitError>,
}
pub(crate) struct HealingKits {
    pub pending: BTreeMap<u64, HealingKitTicket>,
    pub animations: BTreeMap<EntityId, HealingAnimation>,
    pub returns: BTreeMap<EntityId, MotionToken>,
    pub completed_returns: BTreeSet<EntityId>,
    pub outcomes: VecDeque<HealingKitOutcome>,
    pub capacity: usize,
}
impl HealingKits {
    pub fn new(capacity: usize) -> Self {
        Self {
            pending: BTreeMap::new(),
            animations: BTreeMap::new(),
            returns: BTreeMap::new(),
            completed_returns: BTreeSet::new(),
            outcomes: VecDeque::new(),
            capacity: capacity.min(4096),
        }
    }
}

pub(crate) struct HealingAnimation {
    pub correlation: u64,
    pub context: ActionContext,
    pub item: EntityId,
    pub item_revision: u64,
    pub target: EntityId,
    pub profile: PreparedHealingKit,
    pub token: MotionToken,
    pub start_cell: bace_types::CellId,
    pub start_position: bace_geometry::Vec3,
}
