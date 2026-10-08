//! Frozen corpse schema2 preserves the complete V1 death receipt/ownership state
//! and adds explicit placement. Migration requires admitted authoritative pose.
use crate::{CodecLimits, CorpseSaveV1, ItemPlacementV2, SaveCodecError};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseSaveV2 {
    pub corpse: CorpseSaveV1,
    pub placement: ItemPlacementV2,
}
impl CorpseSaveV2 {
    pub fn migrate_v1(
        corpse: CorpseSaveV1,
        placement: ItemPlacementV2,
    ) -> Result<Self, SaveCodecError> {
        let value = Self { corpse, placement };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.corpse.validate()?;
        self.placement.validate(self.corpse.entity.object_id)?;
        if matches!(self.placement, ItemPlacementV2::Contained { .. }) {
            return Err(SaveCodecError::Invalid(
                "corpse cannot be an inventory item",
            ));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(
            102,
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
            102,
            2,
            CodecLimits {
                max_payload_bytes: 2 * 1024 * 1024,
            },
        )?;
        value.validate()?;
        Ok(value)
    }
}
