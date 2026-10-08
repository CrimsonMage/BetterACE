//! Bounded owner-thread commands for trusted pet profiles and authenticated use.
use crate::{InventoryReceipt, InventoryTicket, PetError, PreparedCombatPet, PreparedPassivePet};
use bace_ai::PetUser;
use bace_gameplay_api::ActionContext;
use bace_inventory::ActivationRequirements;
use bace_magic::EnchantmentEntry;
use bace_types::EntityId;
use std::sync::Arc;

pub struct PetCommand {
    pub correlation: u64,
    pub action: PetAction,
}

pub enum PetAction {
    /// Trusted character projection, refreshed after entry and skill changes.
    RegisterOwner { owner: EntityId, user: PetUser },
    Summon {
        context: ActionContext,
        device: EntityId,
        device_revision: u64,
        pet: EntityId,
        user: PetUser,
        activation: Option<ActivationRequirements>,
        profile: Arc<PreparedCombatPet>,
    },
    SummonPassive {
        context: ActionContext,
        device: EntityId,
        device_revision: u64,
        pet: EntityId,
        user: PetUser,
        activation: Option<ActivationRequirements>,
        profile: Arc<PreparedPassivePet>,
    },
    /// Trusted logout request; the inventory receipt still gates retirement.
    Stow { owner: EntityId },
    /// An exact durable receipt, or a definite rollback, settles the reservation.
    Resolve {
        receipt: InventoryReceipt,
        committed: bool,
    },
}

impl PetCommand {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && match &self.action {
                PetAction::RegisterOwner { owner, .. } | PetAction::Stow { owner } => owner.0 != 0,
                PetAction::Summon {
                    context,
                    device,
                    device_revision,
                    pet,
                    ..
                }
                | PetAction::SummonPassive {
                    context,
                    device,
                    device_revision,
                    pet,
                    ..
                } => {
                    context.actor.0 != 0
                        && device.0 != 0
                        && *device_revision != 0
                        && pet.0 >= 0x8000_0000
                }
                PetAction::Resolve { receipt, .. } => {
                    receipt.operation != 0
                        && receipt.revisions.len() <= 1024
                        && receipt
                            .revisions
                            .iter()
                            .all(|(id, revision)| id.0 != 0 && *revision != 0)
                }
            }
    }
}

#[derive(Clone, Debug)]
pub enum PetDecision {
    Registered,
    Proposed {
        ticket: InventoryTicket,
        actor_revision: u64,
        registry_revision: u64,
        registry_after: Vec<EnchantmentEntry>,
    },
    Stowing {
        operation: Option<u64>,
    },
    Resolved {
        operation: u64,
        committed: bool,
    },
}

#[derive(Clone, Debug)]
pub struct PetOutcome {
    pub correlation: u64,
    pub result: Result<PetDecision, PetError>,
}
