//! Trusted immutable inventory inputs and correlated single-owner decisions.
use crate::{InventoryReceipt, InventoryTicket, PreparedStackDrop};
use bace_gameplay_api::{
    ActionContext, CharacterBinding, GeneratorLocation, InventoryRejection, InventoryRequest,
};
use bace_inventory::{InventoryAuthority, StackSplitPreparation};
use bace_magic::EnchantmentEntry;
use bace_types::EntityId;
use std::collections::BTreeMap;

pub struct InventoryPreparedRequest {
    pub context: ActionContext,
    pub request: InventoryRequest,
    /// World-owner evidence; never populate these flags directly from wire data.
    pub authority: InventoryAuthority,
    pub split: Option<StackSplitPreparation>,
    pub drop: Option<Box<PreparedStackDrop>>,
}
pub struct InventoryCommand {
    pub correlation: u64,
    pub kind: InventoryCommandKind,
}
pub enum InventoryCommandKind {
    Propose(Box<InventoryPreparedRequest>),
    /// Live contained-only ingress; ancestry and unequipped state are rechecked by the owner.
    ProposeOwned(Box<InventoryPreparedRequest>),
    InspectLive {
        context: ActionContext,
        request: InventoryRequest,
    },
    ProposeLive(Box<InventoryLivePrepared>),
    ProposeEquipment(Box<crate::PreparedEquipmentRequest>),
    PrepareEquipmentPhysical {
        operation: u64,
        prepared: Box<crate::PreparedEquipmentPhysical>,
    },
    Commit(InventoryReceipt),
    Reject {
        operation: u64,
    },
}
#[derive(Clone, Debug)]
pub struct InventoryOperation {
    pub corpse_decay: Vec<crate::CorpseDecayChange>,
    pub equipment: Option<std::sync::Arc<crate::EquipmentEffectsPatch>>,
    pub equipment_vitals: Option<crate::EquipmentVitalChange>,
    pub equipment_health_update: bool,
    /// ACE selectable-slot combat-mode change, adopted with the equipment receipt.
    pub equipment_mode_after: Option<u32>,
    pub request: InventoryRequest,
    pub binding: CharacterBinding,
    pub actor_revision: u64,
    pub ticket: InventoryTicket,
    pub transient: Vec<EntityId>,
    /// Accepted before-poses and preflighted after-poses, keyed by item identity.
    pub positions: BTreeMap<EntityId, GeneratorLocation>,
    pub world_placement: Option<crate::StackWorldPlacement>,
    pub enchantments: BTreeMap<EntityId, Vec<EnchantmentEntry>>,
}
#[derive(Clone, Debug)]
pub enum InventoryDecision {
    Proposed(Box<InventoryOperation>),
    Inspected(Box<InventoryInspection>),
    EquipmentPrepared(Box<InventoryOperation>),
    MotionStarted,
    Motion(InventoryMotion),
    Committed(InventoryTicket),
    Rejected { operation: u64 },
}
#[derive(Clone, Debug)]
pub struct InventoryOutcome {
    pub correlation: u64,
    pub result: Result<InventoryDecision, InventoryRejection>,
}
impl InventoryCommand {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && match &self.kind {
                InventoryCommandKind::Propose(p) | InventoryCommandKind::ProposeOwned(p) => {
                    p.context.actor.0 != 0
                        && p.context.actor == p.authority.actor
                        && p.split.as_ref().is_none_or(|s| {
                            s.fresh.id.0 != 0
                                && s.fresh.stack > 0
                                && s.fresh.stack <= s.fresh.maximum_stack
                        })
                }
                InventoryCommandKind::InspectLive { context, .. } => context.actor.0 != 0,
                InventoryCommandKind::ProposeLive(p) => {
                    p.evidence.context.actor.0 != 0
                        && p.evidence.rows.len() <= 1024
                        && p.request.context == p.evidence.context
                        && p.request.request == p.evidence.request
                        && p.motions.len() <= 5
                        && p.use_radius.is_finite()
                        && (0.0..=100.0).contains(&p.use_radius)
                        && (!p.constructed_acquisition
                            || matches!(p.evidence.request, InventoryRequest::Move { .. }))
                }
                InventoryCommandKind::ProposeEquipment(p) => {
                    p.request.context.actor.0 != 0
                        && p.effects.items.len() <= 128
                        && p.slots.len() <= 65
                        && p.stance.as_ref().is_none_or(|chain| {
                            chain.motion == 0x8000_003c
                                && chain.speed == 1.
                                && chain.source_transition().is_some_and(|transition| {
                                    transition.after.style == 0x8000_003c
                                        && !transition.continues_cycle
                                })
                        })
                        && p.request.split.as_ref().is_none_or(|split| {
                            matches!(p.request.request, InventoryRequest::SplitToWield { .. })
                                && p.request.authority.new_item == Some(split.fresh.id)
                                && split.fresh.id.0 != 0
                                && split.fresh.stack > 0
                                && split.fresh.stack <= split.fresh.maximum_stack
                        })
                        && p.request.drop.is_none()
                }
                InventoryCommandKind::PrepareEquipmentPhysical {
                    operation,
                    prepared,
                } => {
                    *operation != 0
                        && prepared.motions.len() <= 64
                        && prepared.locomotion_styles.len() <= 4
                        && prepared.death_motions.len() <= 16
                        && prepared.source.equipment.len() <= 64
                        && prepared.source.qualities.len() <= 32768
                }
                InventoryCommandKind::Commit(receipt) => {
                    receipt.operation != 0
                        && !receipt.revisions.is_empty()
                        && receipt.revisions.len() <= 1024
                }
                InventoryCommandKind::Reject { operation } => *operation != 0,
            }
    }
}

#[derive(Clone, Debug)]
pub struct InventoryInspection {
    pub context: ActionContext,
    pub request: InventoryRequest,
    pub epoch: u16,
    pub motion: Option<bace_motion::SourceMotionState>,
    pub target: Option<EntityId>,
    pub target_corpse: bool,
    pub rows: Vec<bace_inventory::InventoryItem>,
}
pub struct InventoryLivePrepared {
    pub evidence: InventoryInspection,
    pub request: InventoryPreparedRequest,
    /// Cold-verified construction companion for each Creature/Cow in the
    /// inspected source tree. Simulation still checks the exact live owner.
    pub constructed_acquisition: bool,
    pub motions: BTreeMap<u32, std::sync::Arc<bace_motion::PreparedMotionChain>>,
    pub drop_shape: Option<std::sync::Arc<bace_physics::CollisionShape>>,
    pub use_radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InventoryMotion {
    pub actor: EntityId,
    pub style: u32,
    pub command: Option<u32>,
}
