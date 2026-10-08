//! Frozen housing V2. Eviction/abandonment retain child items. No owner means no
//! storage access; the next committed owner inherits existing house contents.
use crate::{CodecLimits, EntitySaveV1, HouseAccessV1, HouseSaveV1, SaveCodecError};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HousePaymentV2 {
    pub template: u32,
    pub required: u32,
    pub paid: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HouseSaveV2 {
    pub entity: EntitySaveV1,
    pub house_id: u32,
    pub owner_id: Option<u32>,
    pub purchased_at: i64,
    pub rent_period_start: i64,
    pub rent_due_at: i64,
    pub access_generation: u64,
    pub open_to_all: bool,
    pub storage_open: bool,
    pub hooks_visible: bool,
    pub maintenance_free: bool,
    /// V1 has no payment profile; gameplay admission needs an explicit content supplement.
    pub rent_complete: bool,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub access: Vec<HouseAccessV1>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub rent: Vec<HousePaymentV2>,
}
impl HouseSaveV2 {
    pub fn migrate_v1(old: HouseSaveV1) -> Result<Self, SaveCodecError> {
        old.validate()?;
        Ok(Self {
            entity: old.entity,
            house_id: old.house_id,
            owner_id: Some(old.owner_id),
            purchased_at: old.purchased_at,
            rent_period_start: old.rent_period_start,
            rent_due_at: old.rent_due_at,
            access_generation: 1,
            open_to_all: false,
            storage_open: false,
            hooks_visible: true,
            maintenance_free: false,
            rent_complete: false,
            access: old.access,
            rent: Vec::new(),
        })
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.entity.validate()?;
        if self.house_id == 0
            || self.access_generation == 0
            || self.access_generation > i64::MAX as u64
            || self.owner_id.is_some_and(|o| {
                !(0x50000001..=0x5fffffff).contains(&o) || o == self.entity.object_id
            })
            || self.purchased_at < 0
            || self.rent_period_start < 0
            || self.rent_due_at < self.rent_period_start
            || self.access.len() > 4096
            || self.rent.len() > 256
        {
            return Err(SaveCodecError::Invalid("house V2 identity or bounds"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for a in &self.access {
            if !(0x50000001..=0x5fffffff).contains(&a.player_id) || !ids.insert(a.player_id) {
                return Err(SaveCodecError::Invalid("house V2 access"));
            }
        }
        ids.clear();
        for r in &self.rent {
            if r.template == 0 || r.required == 0 || r.paid > r.required || !ids.insert(r.template)
            {
                return Err(SaveCodecError::Invalid("house V2 rent"));
            }
        }
        if self.owner_id.is_none()
            && (!self.access.is_empty() || self.open_to_all || self.storage_open)
        {
            return Err(SaveCodecError::Invalid("unowned housing grants access"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(
            103,
            2,
            self,
            CodecLimits {
                max_payload_bytes: 2 * 1024 * 1024,
            },
        )?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(
            bytes,
            103,
            2,
            CodecLimits {
                max_payload_bytes: 2 * 1024 * 1024,
            },
        )?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_migrate(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let info = crate::inspect(
            bytes,
            CodecLimits {
                max_payload_bytes: 2 * 1024 * 1024,
            },
        )?;
        match info.schema_version {
            1 => Self::migrate_v1(HouseSaveV1::decode(bytes)?),
            2 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported housing schema")),
        }
    }
}
