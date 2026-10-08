//! Owned messages for the bounded simulation adapter. Prepared device content
//! stays registered with the kernel; clients supply only identity and consent.
use crate::{InventoryReceipt, SkillDeviceConfirmation, SkillDeviceError, SkillDeviceTicket};
use bace_gameplay_api::ActionContext;
use bace_types::EntityId;

#[derive(Clone)]
pub enum SkillDeviceCommand {
    /// The pinned generic OnActivate Active=0 return. Authorize the accepted
    /// item and action, but do not quote, start cooldown or mutate either owner.
    RequestInactive {
        context: ActionContext,
        item: EntityId,
        revision: u64,
    },
    /// Trusted immutable source projections, checked against the live graph before
    /// the original client action is authorized. Not a client wire payload.
    RequestPrepared {
        context: ActionContext,
        item: EntityId,
        revision: u64,
        device: crate::PreparedSkillDevice,
        activation: Option<bace_inventory::ActivationRequirements>,
        cooldown_seconds: Option<f64>,
        wielded: Vec<(
            EntityId,
            u64,
            [Option<bace_character::SkillWieldRequirement>; 4],
        )>,
        lifetime: u64,
    },
    Request {
        context: ActionContext,
        item: EntityId,
        lifetime: u64,
    },
    Confirm {
        context: ActionContext,
        token: u64,
        accept: bool,
    },
    Commit {
        receipt: InventoryReceipt,
    },
    Rollback {
        operation: u64,
    },
}
#[derive(Debug)]
pub enum SkillDeviceResult {
    Inactive,
    Confirmation(SkillDeviceConfirmation),
    Proposed(Option<SkillDeviceTicket>),
    Committed(SkillDeviceTicket),
    RolledBack(u64),
}
#[derive(Debug)]
pub struct SkillDeviceOutcome {
    pub context: Option<ActionContext>,
    pub result: Result<SkillDeviceResult, SkillDeviceError>,
}

impl SkillDeviceCommand {
    pub fn valid_bounds(&self) -> bool {
        match self {
            Self::RequestInactive { item, revision, .. } => item.0 != 0 && *revision != 0,
            Self::RequestPrepared {
                item,
                revision,
                wielded,
                cooldown_seconds,
                lifetime,
                ..
            } => {
                item.0 != 0
                    && *revision != 0
                    && *lifetime > 0
                    && *lifetime <= 1800
                    && cooldown_seconds.is_none_or(|v| v.is_finite() && v > 0.0 && v <= 86400.0)
                    && wielded.len() <= 1023
                    && wielded
                        .iter()
                        .all(|(id, revision, _)| id.0 != 0 && *revision != 0)
            }
            Self::Request { item, lifetime, .. } => {
                item.0 != 0 && *lifetime > 0 && *lifetime <= 1800
            }
            Self::Confirm { token, .. } => *token > 0 && *token <= u64::from(u32::MAX),
            Self::Commit { receipt } => receipt.operation != 0 && receipt.revisions.len() <= 1024,
            Self::Rollback { operation } => *operation != 0,
        }
    }
}
