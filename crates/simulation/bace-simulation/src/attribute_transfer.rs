//! Bounded source-prepared AttributeTransferDevice confirmation and exact
//! character+item transaction state. Pinned ACE uses Int189/190 and type 63.
use crate::{InventoryReceipt, InventoryTicket, characters::AttributeTransferTicket};
use bace_character::AttributeTransferError;
use bace_gameplay_api::{ActionContext, AttributeId, InventoryRejection};
use bace_inventory::ActivationRequirements;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedAttributeTransfer {
    pub from: AttributeId,
    pub to: AttributeId,
}
impl PreparedAttributeTransfer {
    pub fn source(from: i32, to: i32) -> Result<Self, AttributeTransferDeviceError> {
        Ok(Self {
            from: AttributeId::try_from(
                u32::try_from(from).map_err(|_| AttributeTransferDeviceError::Content)?,
            )
            .map_err(|_| AttributeTransferDeviceError::Content)?,
            to: AttributeId::try_from(
                u32::try_from(to).map_err(|_| AttributeTransferDeviceError::Content)?,
            )
            .map_err(|_| AttributeTransferDeviceError::Content)?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttributeTransferConfirmation {
    pub token: u64,
    pub actor: EntityId,
    pub item: EntityId,
    pub expires: u64,
    pub device: PreparedAttributeTransfer,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AttributeTransferDeviceTicket {
    pub character: AttributeTransferTicket,
    pub inventory: InventoryTicket,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeTransferDeviceError {
    Olthoi,
    Content,
    Ownership,
    Busy,
    Capacity,
    Confirmation,
    Expired,
    Stale,
    MissingWieldProfile,
    Activation(bace_inventory::ActivationFailure),
    Domain(AttributeTransferError),
    Character(crate::characters::AttributeTransferActionError),
    Inventory(InventoryRejection),
}
#[derive(Clone, Debug)]
pub enum AttributeTransferCommand {
    RequestInactive {
        context: ActionContext,
        item: EntityId,
        revision: u64,
    },
    RequestPrepared {
        context: ActionContext,
        item: EntityId,
        revision: u64,
        device: PreparedAttributeTransfer,
        activation: Option<ActivationRequirements>,
        /// Every equipped item, with current revision and whether one of its
        /// four authored WieldRequirement slots is Attrib or RawAttrib.
        wielded: Vec<(EntityId, u64, bool)>,
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
impl AttributeTransferCommand {
    pub fn valid_bounds(&self) -> bool {
        match self {
            Self::RequestInactive { item, revision, .. } => item.0 != 0 && *revision != 0,
            Self::RequestPrepared {
                item,
                revision,
                wielded,
                lifetime,
                ..
            } => {
                item.0 != 0
                    && *revision != 0
                    && *lifetime > 0
                    && *lifetime <= 1800
                    && wielded.len() <= 1023
                    && wielded
                        .iter()
                        .all(|(id, revision, _)| id.0 != 0 && *revision != 0)
            }
            Self::Confirm { token, .. } => *token > 0 && *token <= u64::from(u32::MAX),
            Self::Commit { receipt } => receipt.operation != 0 && receipt.revisions.len() <= 1024,
            Self::Rollback { operation } => *operation != 0,
        }
    }
}
#[derive(Debug)]
pub enum AttributeTransferResult {
    Inactive,
    Confirmation(AttributeTransferConfirmation),
    Proposed(Option<AttributeTransferDeviceTicket>),
    Committed(AttributeTransferDeviceTicket),
    RolledBack(u64),
}
#[derive(Debug)]
pub struct AttributeTransferOutcome {
    pub context: Option<ActionContext>,
    pub result: Result<AttributeTransferResult, AttributeTransferDeviceError>,
}

#[derive(Clone)]
pub(crate) struct DeviceProfile {
    pub revision: u64,
    pub device: PreparedAttributeTransfer,
    pub activation: Option<ActivationRequirements>,
}
#[derive(Clone, Copy)]
pub(crate) struct Confirmation {
    pub public: AttributeTransferConfirmation,
    pub context: ActionContext,
    pub revision: u64,
}
pub(crate) struct AttributeTransfers {
    pub devices: BTreeMap<EntityId, DeviceProfile>,
    pub wielded: BTreeMap<EntityId, (u64, bool)>,
    pub confirmations: BTreeMap<EntityId, Confirmation>,
    pub pending: BTreeMap<u64, AttributeTransferDeviceTicket>,
    pub outbox: VecDeque<u64>,
    pub submitted: BTreeSet<u64>,
    pub next: u64,
    pub capacity: usize,
}
impl AttributeTransfers {
    pub fn new(capacity: usize) -> Self {
        Self {
            devices: BTreeMap::new(),
            wielded: BTreeMap::new(),
            confirmations: BTreeMap::new(),
            pending: BTreeMap::new(),
            outbox: VecDeque::new(),
            submitted: BTreeSet::new(),
            next: 0,
            capacity: capacity.min(4096),
        }
    }
}
