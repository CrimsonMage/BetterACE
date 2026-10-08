//! Frozen, non-world marker for an accepted PVE death. It gives a zero-drop
//! NoCorpse transaction a durable exact-once identity without fabricating an
//! item or creature during region restoration.
use crate::{CodecLimits, ItemPlacementV2, SaveCodecError};
use bace_content::Position;
use serde::{Deserialize, Serialize};

pub const PVE_DEATH_RECEIPT_KIND: u16 = 104;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PveDeathReceiptV1 {
    pub marker_object_id: u32,
    pub event_id: [u8; 16],
    pub victim_object_id: u32,
    /// Exact accepted death pose; no mutable source Position is borrowed.
    pub position: Position,
    pub world_epoch: u64,
    pub killed: bool,
}

impl PveDeathReceiptV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.marker_object_id < 0x8000_0000
            || self.victim_object_id == 0
            || self.victim_object_id == self.marker_object_id
            || self.event_id == [0; 16]
            || self.world_epoch == 0
            || !self.killed
        {
            return Err(SaveCodecError::Invalid("PVE death receipt identity"));
        }
        ItemPlacementV2::World(self.position.clone()).validate(self.marker_object_id)
    }

    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(PVE_DEATH_RECEIPT_KIND, 1, self, limits())?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let receipt: Self = crate::decode(bytes, PVE_DEATH_RECEIPT_KIND, 1, limits())?;
        receipt.validate()?;
        Ok(receipt)
    }
}

fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 1024,
    }
}
