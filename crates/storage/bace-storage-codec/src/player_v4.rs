//! Frozen player schema4. Recovery uses an explicit trusted capture clock;
//! migrating older saves never invents a prior cast or cooldown.
use crate::{
    CodecLimits, FrozenCombatRecoveryV1, PlayerSaveV1, PlayerSaveV2, PlayerSaveV3, SaveCodecError,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRecoverySaveV1 {
    pub captured_unix_millis: u64,
    pub state: FrozenCombatRecoveryV1,
}
impl CombatRecoverySaveV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.captured_unix_millis > i64::MAX as u64 {
            return Err(SaveCodecError::Invalid("combat recovery capture time"));
        }
        self.state.validate()
    }
    pub fn elapsed_seconds(&self, now_unix_millis: u64) -> Result<f64, SaveCodecError> {
        self.validate()?;
        if now_unix_millis > i64::MAX as u64 {
            return Err(SaveCodecError::Invalid("combat recovery clock"));
        }
        // A backwards wall-clock correction cannot clear or shorten a lock.
        Ok(now_unix_millis.saturating_sub(self.captured_unix_millis) as f64 / 1000.0)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSaveV1 {
    pub id: u32,
    pub display: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSaveV4 {
    pub previous: PlayerSaveV3,
    pub combat_recovery: Option<CombatRecoverySaveV1>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub contracts: Vec<ContractSaveV1>,
}
impl std::ops::Deref for PlayerSaveV4 {
    type Target = PlayerSaveV3;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for PlayerSaveV4 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl PlayerSaveV4 {
    pub fn migrate_v1(value: PlayerSaveV1) -> Result<Self, SaveCodecError> {
        Self::migrate_v3(PlayerSaveV3::migrate_v1(value)?)
    }
    pub fn migrate_v2(value: PlayerSaveV2) -> Result<Self, SaveCodecError> {
        Self::migrate_v3(PlayerSaveV3::migrate_v2(value)?)
    }
    pub fn migrate_v3(previous: PlayerSaveV3) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            combat_recovery: None,
            contracts: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        if self.contracts.len() > 100
            || self.contracts.iter().any(|c| c.id == 0)
            || self.contracts.windows(2).any(|c| c[0].id >= c[1].id)
            || self.contracts.iter().filter(|c| c.display).count() > 1
        {
            return Err(SaveCodecError::Invalid("contract registry"));
        }
        if let Some(value) = self.combat_recovery {
            value.validate()?;
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(100, 4, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 100, 4, limits())?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_or_migrate(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let header = crate::inspect(bytes, limits())?;
        if header.kind != 100 {
            return Err(SaveCodecError::Invalid("player save kind"));
        }
        match header.schema_version {
            1..=3 => Self::migrate_v3(PlayerSaveV3::decode_or_migrate(bytes)?),
            4 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported player schema")),
        }
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}
