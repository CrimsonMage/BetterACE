//! Frozen schema6 preserves physical recovery across ownership transfers.
use crate::{CodecLimits, PlayerSaveV5, SaveCodecError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalRecoverySaveV1 {
    pub captured_unix_millis: u64,
    pub remaining_seconds: f64,
}
impl PhysicalRecoverySaveV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.captured_unix_millis > i64::MAX as u64
            || !self.remaining_seconds.is_finite()
            || !(0.0..=180.0).contains(&self.remaining_seconds)
        {
            return Err(SaveCodecError::Invalid("physical recovery bounds"));
        }
        Ok(())
    }
    pub fn remaining_at(&self, now_unix_millis: u64) -> Result<f64, SaveCodecError> {
        self.validate()?;
        if now_unix_millis > i64::MAX as u64 {
            return Err(SaveCodecError::Invalid("physical recovery clock"));
        }
        let elapsed = now_unix_millis.saturating_sub(self.captured_unix_millis) as f64 / 1000.0;
        Ok((self.remaining_seconds - elapsed).max(0.0))
    }
}
/// A same-revision capture may age conservatively, but cannot reset a lock or
/// clear it faster than the explicitly supplied trusted elapsed clock.
pub fn validate_physical_recovery_transition(
    before: Option<PhysicalRecoverySaveV1>,
    after: Option<PhysicalRecoverySaveV1>,
    revision_changed: bool,
) -> Result<(), SaveCodecError> {
    if let Some(after) = after {
        after.validate()?;
    }
    let Some(before) = before else {
        if !revision_changed && after.is_some_and(|r| r.remaining_seconds > 0.0) {
            return Err(SaveCodecError::Invalid(
                "physical recovery requires dirty revision",
            ));
        }
        return Ok(());
    };
    before.validate()?;
    let after = after.ok_or(SaveCodecError::Invalid("cannot erase physical recovery"))?;
    if after.captured_unix_millis < before.captured_unix_millis {
        return Err(SaveCodecError::Invalid("physical recovery clock rewind"));
    }
    if !revision_changed
        && (after.remaining_seconds > before.remaining_seconds + 0.001000001
            || after.remaining_seconds + 0.001000001
                < before.remaining_at(after.captured_unix_millis)?)
    {
        return Err(SaveCodecError::Invalid(
            "physical recovery changed without revision",
        ));
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSaveV6 {
    pub previous: PlayerSaveV5,
    pub physical_recovery: Option<PhysicalRecoverySaveV1>,
}
impl std::ops::Deref for PlayerSaveV6 {
    type Target = PlayerSaveV5;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for PlayerSaveV6 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl PlayerSaveV6 {
    pub fn migrate_v1(previous: crate::PlayerSaveV1) -> Result<Self, SaveCodecError> {
        Self::migrate_v5(PlayerSaveV5::migrate_v1(previous)?)
    }
    pub fn migrate_v2(previous: crate::PlayerSaveV2) -> Result<Self, SaveCodecError> {
        Self::migrate_v5(PlayerSaveV5::migrate_v2(previous)?)
    }
    pub fn migrate_v3(previous: crate::PlayerSaveV3) -> Result<Self, SaveCodecError> {
        Self::migrate_v5(PlayerSaveV5::migrate_v3(previous)?)
    }
    pub fn migrate_v4(previous: crate::PlayerSaveV4) -> Result<Self, SaveCodecError> {
        Self::migrate_v5(PlayerSaveV5::migrate_v4(previous)?)
    }
    pub fn migrate_v5(previous: PlayerSaveV5) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            physical_recovery: None,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        if let Some(recovery) = self.physical_recovery {
            recovery.validate()?;
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(100, 6, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 100, 6, limits())?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_or_migrate(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let header = crate::inspect(bytes, limits())?;
        if header.kind != 100 {
            return Err(SaveCodecError::Invalid("player save kind"));
        }
        match header.schema_version {
            1..=5 => Self::migrate_v5(PlayerSaveV5::decode_or_migrate(bytes)?),
            6 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported player schema")),
        }
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}
