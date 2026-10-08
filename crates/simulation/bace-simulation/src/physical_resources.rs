//! Typed ammunition durability requests. Only the trusted runtime may resolve a
//! reserved proposal after its exact private database receipt is known.
use bace_gameplay_api::{
    CharacterBinding, InventoryRejection, weapon_combat::PhysicalLaunchProposal,
};
use bace_types::EntityId;
use std::sync::Arc;
pub struct PhysicalResourceCommand {
    pub correlation: u64,
    pub action: PhysicalResourceAction,
}
pub enum PhysicalResourceAction {
    Prepare {
        binding: CharacterBinding,
        launch: Box<PhysicalLaunchProposal>,
    },
    Resolve {
        operation: u64,
        committed: bool,
    },
    Acknowledge {
        operation: u64,
    },
    Abort {
        binding: CharacterBinding,
        operation: u64,
    },
    SupplyProjectiles(Vec<EntityId>),
}
impl PhysicalResourceCommand {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && match &self.action {
                PhysicalResourceAction::Prepare { binding, launch } => {
                    binding.actor.0 != 0
                        && binding.actor.0 == launch.actor
                        && launch.operation != 0
                        && launch.consumed == 1
                        && launch.expected_count != 0
                        && launch.event_id != [0; 16]
                }
                PhysicalResourceAction::Resolve { operation, .. }
                | PhysicalResourceAction::Acknowledge { operation } => *operation != 0,
                PhysicalResourceAction::Abort { binding, operation } => {
                    binding.actor.0 != 0 && *operation != 0
                }
                PhysicalResourceAction::SupplyProjectiles(ids) => {
                    ids.len() <= 64
                        && ids.iter().all(|id| id.0 != 0)
                        && ids.windows(2).all(|p| p[0] < p[1])
                }
            }
    }
}
#[derive(Clone, Debug)]
pub struct PhysicalResourceTicket {
    pub appearance: Option<Arc<bace_content::WeenieV1>>,
    pub binding: CharacterBinding,
    pub launch: PhysicalLaunchProposal,
    pub inventory: crate::InventoryTicket,
    pub actor_revision: u64,
    pub snapshot: Arc<crate::PlayerReadSnapshot>,
}
#[derive(Debug)]
pub enum PhysicalResourceDecision {
    Prepared(Arc<PhysicalResourceTicket>),
    Resolved { operation: u64, committed: bool },
    Acknowledged { operation: u64 },
    Aborted { operation: u64 },
    Projectiles { remaining: usize },
}
#[derive(Debug)]
pub struct PhysicalResourceOutcome {
    pub correlation: u64,
    pub result: Result<PhysicalResourceDecision, InventoryRejection>,
}
pub(crate) struct PhysicalResourcePending {
    pub binding: CharacterBinding,
    pub launch: PhysicalLaunchProposal,
    pub inventory: crate::InventoryTicket,
    pub actor_revision: u64,
    pub ticket: Option<Arc<PhysicalResourceTicket>>,
    pub phase: PhysicalResourcePhase,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalResourcePhase {
    Reserved,
    InventoryCommitted,
    Launched,
    Rejected,
}
