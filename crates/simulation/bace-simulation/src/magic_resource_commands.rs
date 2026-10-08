//! Exact component/mana durability lane. The owner retains the pending operation;
//! adapters receive immutable snapshots and return only its durable decision.
use crate::{InventoryTicket, MagicResourceCommit, PlayerReadSnapshot};
use bace_gameplay_api::{CharacterBinding, InventoryRejection};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MagicResourceAction {
    Inspect { cast: u64 },
    Resolve { operation: u64, committed: bool },
    Acknowledge { operation: u64 },
    SupplyPortalIds(Vec<bace_types::EntityId>),
    SupplyProjectileIds(Vec<bace_types::EntityId>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MagicResourceCommand {
    pub correlation: u64,
    pub binding: Option<CharacterBinding>,
    pub action: MagicResourceAction,
}
pub struct PreparedMagicResources {
    pub inventory: InventoryTicket,
    pub resources: MagicResourceCommit,
    pub snapshot: Arc<PlayerReadSnapshot>,
}
pub enum MagicResourceResult {
    Prepared(Arc<PreparedMagicResources>),
    Resolved { operation: u64, committed: bool },
    Acknowledged { operation: u64 },
    PortalIds { remaining: usize, capacity: usize },
    ProjectileIds { remaining: usize, capacity: usize },
}
pub struct MagicResourceOutcome {
    pub correlation: u64,
    pub binding: Option<CharacterBinding>,
    pub result: Result<MagicResourceResult, InventoryRejection>,
}
pub(crate) struct MagicResourceCommands {
    pub(crate) outcomes: VecDeque<Arc<MagicResourceOutcome>>,
    pub(crate) capacity: usize,
    pub(crate) pending: BTreeMap<u64, MagicResourcePending>,
}
impl MagicResourceCommands {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            outcomes: VecDeque::with_capacity(capacity),
            capacity,
            pending: BTreeMap::new(),
        }
    }
}

pub(crate) struct MagicResourcePending {
    pub(crate) binding: CharacterBinding,
    pub(crate) ticket: Arc<PreparedMagicResources>,
    pub(crate) resolved: Option<bool>,
}

impl MagicResourceCommand {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && match &self.action {
                MagicResourceAction::SupplyPortalIds(ids)
                | MagicResourceAction::SupplyProjectileIds(ids) => {
                    self.binding.is_none()
                        && ids.len() <= 64
                        && ids.windows(2).all(|p| p[0] < p[1])
                        && ids
                            .iter()
                            .all(|id| (0x80000000..=0xfffffffe).contains(&id.0))
                }
                _ => self
                    .binding
                    .is_some_and(|b| b.actor.0 != 0 && b.account.0 != 0 && b.session.0 != 0),
            }
    }
}
