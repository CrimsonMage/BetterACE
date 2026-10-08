//! Frozen corpse identity supplement. Legacy saves retain an explicitly unknown
//! source/operation; runtime admission must resolve that absence before adoption.
use crate::{CodecLimits, CorpseSaveV3, ItemPlacementV2, SaveCodecError};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseSaveV4 {
    pub previous: CorpseSaveV3,
    pub source: Option<u32>,
    pub operation: Option<u64>,
}
impl std::ops::Deref for CorpseSaveV4 {
    type Target = CorpseSaveV3;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for CorpseSaveV4 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl CorpseSaveV4 {
    pub fn migrate_v3(previous: CorpseSaveV3) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            source: None,
            operation: None,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        match (self.source, self.operation) {
            (None, None) | (Some(1..), Some(1..)) => Ok(()),
            _ => Err(SaveCodecError::Invalid("corpse source/operation pair")),
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(102, 4, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 102, 4, limits())?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_or_migrate(
        bytes: &[u8],
        placement: Option<ItemPlacementV2>,
    ) -> Result<Self, SaveCodecError> {
        let info = crate::inspect(bytes, limits())?;
        if info.kind != 102 {
            return Err(SaveCodecError::Invalid("save kind"));
        }
        match info.schema_version {
            1..=3 => Self::migrate_v3(CorpseSaveV3::decode_or_migrate(bytes, placement)?),
            4 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported save schema")),
        }
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}

/// Death identity is immutable. Expiry may shorten at a strictly newer mutation
/// revision (the final-item pickup transaction), but may never extend. A legacy save
/// may gain authoritative missing source identity, but cannot erase known fields.
pub fn validate_corpse_transition(
    before: &CorpseSaveV4,
    after: &CorpseSaveV4,
) -> Result<(), SaveCodecError> {
    before.validate()?;
    after.validate()?;
    if before.corpse.entity.object_id != after.corpse.entity.object_id
        || before.corpse.owner != after.corpse.owner
        || before.corpse.death_operation != after.corpse.death_operation
        || after.corpse.expires_at > before.corpse.expires_at
        || after.corpse.expires_at < before.corpse.expires_at
            && after.corpse.entity.mutation_revision <= before.corpse.entity.mutation_revision
        || before.source.is_some()
            && (before.source != after.source || before.operation != after.operation)
    {
        return Err(SaveCodecError::Invalid(
            "corpse death identity/expiry changed",
        ));
    }
    Ok(())
}
