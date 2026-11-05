//! Exact item-mana proposals retained until an inventory placement receipt.
use bace_gameplay_api::ActionContext;
use bace_inventory::{ManaStoneDecision, ManaStoneItem, ManaStoneProfile};
use std::collections::{BTreeMap, VecDeque};

use crate::{InventoryReceipt, InventoryTicket};

#[derive(Clone, Debug)]
pub enum ManaStoneCommand {
    Use {
        correlation: u64,
        context: ActionContext,
        stone: ManaStoneItem,
        stone_revision: u64,
        target: ManaStoneItem,
        target_revision: u64,
        profile: ManaStoneProfile,
    },
    Commit {
        correlation: u64,
        receipt: InventoryReceipt,
    },
    Reject {
        correlation: u64,
        operation: u64,
    },
}
impl ManaStoneCommand {
    pub fn correlation(&self) -> u64 {
        match self {
            Self::Use { correlation, .. }
            | Self::Commit { correlation, .. }
            | Self::Reject { correlation, .. } => *correlation,
        }
    }
    pub fn valid_bounds(&self) -> bool {
        match self {
            Self::Use {
                correlation,
                context,
                stone,
                stone_revision,
                target,
                target_revision,
                ..
            } => {
                *correlation != 0
                    && context.actor.0 != 0
                    && stone.id.0 != 0
                    && *stone_revision != 0
                    && target.id.0 != 0
                    && *target_revision != 0
            }
            Self::Commit {
                correlation,
                receipt,
            } => {
                *correlation != 0
                    && receipt.operation != 0
                    && !receipt.revisions.is_empty()
                    && receipt.revisions.len() <= 1024
            }
            Self::Reject {
                correlation,
                operation,
            } => *correlation != 0 && *operation != 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ManaStoneTicket {
    pub context: ActionContext,
    pub actor_revision: u64,
    pub inventory: InventoryTicket,
    pub decision: ManaStoneDecision,
}

#[derive(Clone, Debug)]
pub enum ManaStoneResult {
    Immediate(ManaStoneDecision),
    Proposed(ManaStoneTicket),
    Committed(ManaStoneTicket),
    Rejected(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManaStoneError {
    NotBound,
    Ownership,
    Stale,
    Busy,
    InvalidSource,
    InvalidTarget,
    NeedsMana,
    Capacity,
    Receipt,
}

#[derive(Clone, Debug)]
pub struct ManaStoneOutcome {
    pub correlation: u64,
    pub context: Option<ActionContext>,
    pub result: Result<ManaStoneResult, ManaStoneError>,
}

pub(crate) struct ManaStones {
    pub(crate) pending: BTreeMap<u64, ManaStoneTicket>,
    pub(crate) outcomes: VecDeque<ManaStoneOutcome>,
    pub(crate) capacity: usize,
}
impl ManaStones {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            pending: BTreeMap::new(),
            outcomes: VecDeque::with_capacity(capacity.min(4096)),
            capacity: capacity.min(4096),
        }
    }
    pub(crate) fn owns(&self, operation: u64) -> bool {
        self.pending.contains_key(&operation)
    }
}
