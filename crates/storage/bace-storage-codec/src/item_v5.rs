//! Frozen source CreateList destination for death treasure eligibility.
//! Legacy items retain unknown origin rather than inferring it from placement.
use crate::{CodecLimits, ItemPlacementV2, ItemSaveV4, SaveCodecError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSaveV5 {
    pub previous: ItemSaveV4,
    /// Pinned ACE DestinationType flags from the acquisition CreateList row.
    /// None means that the origin was not durably recorded.
    pub source_destination: Option<u8>,
}

impl std::ops::Deref for ItemSaveV5 {
    type Target = ItemSaveV4;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}

impl std::ops::DerefMut for ItemSaveV5 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}

impl ItemSaveV5 {
    pub fn migrate_v4(previous: ItemSaveV4) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            source_destination: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        if self
            .source_destination
            .is_some_and(|flags| flags & !0x3f != 0)
        {
            return Err(SaveCodecError::Invalid("item source destination flags"));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(101, 5, self, limits())?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 101, 5, limits())?;
        value.validate()?;
        Ok(value)
    }

    pub fn decode_or_migrate(
        bytes: &[u8],
        placement: Option<ItemPlacementV2>,
    ) -> Result<Self, SaveCodecError> {
        let header = crate::inspect(bytes, limits())?;
        if header.kind != 101 {
            return Err(SaveCodecError::Invalid("save kind"));
        }
        match header.schema_version {
            1..=4 => Self::migrate_v4(ItemSaveV4::decode_or_migrate(bytes, placement)?),
            5 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported save schema")),
        }
    }
}

/// The acquisition source is immutable once known. A legacy unknown origin
/// cannot be filled in by an ordinary item mutation or a placement guess.
pub fn validate_item_source_destination_transition(
    before: &ItemSaveV5,
    after: &ItemSaveV5,
) -> Result<(), SaveCodecError> {
    before.validate()?;
    after.validate()?;
    if before.entity.object_id != after.entity.object_id
        || before.source_destination != after.source_destination
    {
        return Err(SaveCodecError::Invalid("item source destination changed"));
    }
    crate::validate_item_construction_transition(&before.previous, &after.previous)
}

fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}
