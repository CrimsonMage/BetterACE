//! Frozen player schema5 adds persistent social preferences without resetting prior state.
use crate::{
    CodecLimits, PlayerSaveV1, PlayerSaveV2, PlayerSaveV3, PlayerSaveV4, SaveCodecError,
    SocialSaveV1,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSaveV5 {
    pub previous: PlayerSaveV4,
    pub social: SocialSaveV1,
}
impl std::ops::Deref for PlayerSaveV5 {
    type Target = PlayerSaveV4;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for PlayerSaveV5 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl PlayerSaveV5 {
    pub fn migrate_v1(value: PlayerSaveV1) -> Result<Self, SaveCodecError> {
        Self::migrate_v4(PlayerSaveV4::migrate_v1(value)?)
    }
    pub fn migrate_v2(value: PlayerSaveV2) -> Result<Self, SaveCodecError> {
        Self::migrate_v4(PlayerSaveV4::migrate_v2(value)?)
    }
    pub fn migrate_v3(value: PlayerSaveV3) -> Result<Self, SaveCodecError> {
        Self::migrate_v4(PlayerSaveV4::migrate_v3(value)?)
    }
    pub fn migrate_v4(previous: PlayerSaveV4) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            social: Default::default(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        self.social.validate()
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(100, 5, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 100, 5, limits())?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_or_migrate(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let header = crate::inspect(bytes, limits())?;
        if header.kind != 100 {
            return Err(SaveCodecError::Invalid("player save kind"));
        }
        match header.schema_version {
            1..=4 => Self::migrate_v4(PlayerSaveV4::decode_or_migrate(bytes)?),
            5 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported player schema")),
        }
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}
