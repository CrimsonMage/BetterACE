//! Frozen schema 2 adds explicit durable placement. V1 migration requires trusted
//! relational context: missing containment must never be guessed to mean deletion.
use crate::{CodecLimits, EntitySaveV1, SaveCodecError};
use bace_content::Position;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ItemPlacementV2 {
    Contained {
        container: u32,
        slot: u32,
        pack_slot: bool,
        equipped: u32,
    },
    World(Position),
    Removed,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSaveV2 {
    pub entity: EntitySaveV1,
    pub placement: ItemPlacementV2,
}
impl ItemPlacementV2 {
    pub fn validate(&self, item: u32) -> Result<(), SaveCodecError> {
        match self {
            Self::Contained {
                container,
                pack_slot,
                equipped,
                ..
            } => {
                if *container == 0
                    || *container == u32::MAX
                    || *container == item
                    || *pack_slot && *equipped != 0
                {
                    return Err(SaveCodecError::Invalid("item containment"));
                }
            }
            Self::World(p) => {
                let values = [
                    p.position_x,
                    p.position_y,
                    p.position_z,
                    p.rotation_w,
                    p.rotation_x,
                    p.rotation_y,
                    p.rotation_z,
                ];
                let norm = p.rotation_w * p.rotation_w
                    + p.rotation_x * p.rotation_x
                    + p.rotation_y * p.rotation_y
                    + p.rotation_z * p.rotation_z;
                if p.obj_cell_id == 0
                    || values.iter().any(|v| !v.is_finite())
                    || (norm - 1.0).abs() > 0.01
                {
                    return Err(SaveCodecError::Invalid("item world placement"));
                }
            }
            Self::Removed => {}
        }
        Ok(())
    }
}
impl ItemSaveV2 {
    pub fn migrate_v1(
        entity: EntitySaveV1,
        placement: ItemPlacementV2,
    ) -> Result<Self, SaveCodecError> {
        let result = Self { entity, placement };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.entity.validate()?;
        self.placement.validate(self.entity.object_id)
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(
            101,
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
            101,
            2,
            CodecLimits {
                max_payload_bytes: 2 * 1024 * 1024,
            },
        )?;
        value.validate()?;
        Ok(value)
    }
}
