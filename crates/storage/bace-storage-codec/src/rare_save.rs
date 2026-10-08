//! Frozen character-specific rare state V1. Not a gameplay struct alias.
use crate::SaveCodecError;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RareStateV1 {
    pub character: u32,
    pub random_identity: [u8; 16],
    pub key_version: u32,
    pub attempt_ordinal: u64,
    pub timer_ordinal: u64,
    pub next_realtime_at: Option<u64>,
    pub last_effective_time: u64,
}
impl RareStateV1 {
    pub fn validate(&self, character: u32) -> Result<(), SaveCodecError> {
        if self.character != character
            || self.random_identity == [0; 16]
            || self.key_version == 0
            || self.next_realtime_at.is_some_and(|t| t > i64::MAX as u64)
            || self.last_effective_time > i64::MAX as u64
        {
            return Err(SaveCodecError::Invalid(
                "rare character identity, key or timestamp",
            ));
        }
        Ok(())
    }
}
