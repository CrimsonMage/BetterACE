//! Bounded confirmations for source-prepared skill devices. Content profiles are
//! immutable inputs; inventory and character aggregates remain the sole owners.
//! Source: pinned ACE SkillAlterationDevice.cs and AugmentationDevice.cs.
use bace_character::{SkillTransitionError, SkillWieldRequirement};
use bace_gameplay_api::{ActionContext, InventoryRejection};
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedSkillDevice {
    Specialize(u32),
    Lower(u32),
    Augment { skill: u32, experience_cost: u64 },
}
impl PreparedSkillDevice {
    /// Trusted content properties Int185 and Int186, never client fields.
    pub fn alteration(kind: i32, skill: i32) -> Result<Self, SkillDeviceError> {
        if !(1..=54).contains(&skill) {
            return Err(SkillDeviceError::Content);
        }
        match kind {
            1 => Ok(Self::Specialize(skill as u32)),
            2 => Ok(Self::Lower(skill as u32)),
            _ => Err(SkillDeviceError::Content),
        }
    }
    /// Trusted content Int215 (AugmentationStat), Int64#3 (AugmentationCost).
    pub fn augmentation(kind: i32, cost: i64) -> Result<Self, SkillDeviceError> {
        let skill = match kind {
            7 => 40,
            8 => 18,
            9 => 29,
            10 => 30,
            11 => 28,
            _ => return Err(SkillDeviceError::Content),
        };
        Ok(Self::Augment {
            skill,
            experience_cost: u64::try_from(cost).map_err(|_| SkillDeviceError::Content)?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillDeviceConfirmation {
    pub token: u64,
    pub actor: EntityId,
    pub item: EntityId,
    pub expires: u64,
    pub device: PreparedSkillDevice,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SkillDeviceTicket {
    pub skill: crate::SkillTicket,
    pub inventory: crate::InventoryTicket,
    pub cooldown: Option<SkillDeviceCooldown>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SkillDeviceCooldown {
    pub group: u16,
    pub seconds: f64,
    pub before_revision: u64,
    pub after_revision: u64,
    pub after: Vec<bace_magic::EnchantmentEntry>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillDeviceError {
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
    Domain(SkillTransitionError),
    Skill(crate::SkillActionError),
    Inventory(InventoryRejection),
}
#[derive(Clone)]
pub(crate) struct DeviceProfile {
    pub revision: u64,
    pub device: PreparedSkillDevice,
    pub activation: Option<bace_inventory::ActivationRequirements>,
    pub cooldown_seconds: Option<f64>,
}
#[derive(Clone, Copy)]
pub(crate) struct WieldProfile {
    pub revision: u64,
    pub requirements: [Option<SkillWieldRequirement>; 4],
}
#[derive(Clone, Copy)]
pub(crate) struct Confirmation {
    pub public: SkillDeviceConfirmation,
    pub context: ActionContext,
    pub revision: u64,
}
pub(crate) struct SkillDevices {
    pub devices: BTreeMap<EntityId, DeviceProfile>,
    pub wielded: BTreeMap<EntityId, WieldProfile>,
    pub confirmations: BTreeMap<EntityId, Confirmation>,
    pub pending: BTreeMap<u64, SkillDeviceTicket>,
    pub outbox: VecDeque<u64>,
    pub submitted: BTreeSet<u64>,
    pub next: u64,
    pub capacity: usize,
}
impl SkillDevices {
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
