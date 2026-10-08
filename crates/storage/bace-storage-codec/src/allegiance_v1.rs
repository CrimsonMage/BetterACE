//! Frozen per-character allegiance nodes and per-monarch metadata. Relational edges are DB constraints.
use crate::{CodecLimits, SaveCodecError};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllegianceNodeV1 {
    pub character: u32,
    pub account: u64,
    pub name: String,
    pub gender: u8,
    pub heritage: u8,
    pub patron: Option<u32>,
    pub monarch: u32,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub vassals: Vec<u32>,
    pub rank: u32,
    pub followers: u32,
    pub level: u32,
    pub leadership: u32,
    pub loyalty: u32,
    pub sworn_at: u64,
    pub online_seconds: u64,
    pub may_pass_up: bool,
    pub received_total: u64,
    pub tithed_total: u64,
    pub unclaimed: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllegianceSanctuaryV1 {
    pub cell: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllegianceMetadataV1 {
    pub monarch: u32,
    pub chat_room: u32,
    pub name: Option<String>,
    pub motd: Option<String>,
    pub motd_set_by: Option<String>,
    pub officer_titles: [Option<String>; 3],
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub officers: Vec<(u32, u32)>,
    pub locked: bool,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub approved: Vec<u32>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub banned_characters: Vec<(u32, String)>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub chat_gags: Vec<(u32, i64)>,
    pub sanctuary: Option<AllegianceSanctuaryV1>,
}
impl AllegianceNodeV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        let mut ids = std::collections::BTreeSet::new();
        if self.character == 0
            || self.account == 0
            || self.account > i64::MAX as u64
            || self.monarch == 0
            || self.patron == Some(0)
            || self.patron == Some(self.character)
            || self.name.is_empty()
            || self.name.len() > 100
            || self.vassals.len() > 11
            || self
                .vassals
                .iter()
                .any(|id| *id == 0 || *id == self.character || !ids.insert(*id))
            || !(1..=10).contains(&self.rank)
            || !(1..=275).contains(&self.level)
            || self.unclaimed > u64::from(u32::MAX)
        {
            return Err(SaveCodecError::Invalid("allegiance node"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(115, 1, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 115, 1, limits())?;
        value.validate()?;
        Ok(value)
    }
}
impl AllegianceMetadataV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.monarch == 0
            || self.chat_room == 0
            || self.name.as_ref().is_some_and(|s| s.len() > 1024)
            || self.motd.as_ref().is_some_and(|s| s.len() > 4096)
            || self.motd_set_by.as_ref().is_some_and(|s| s.len() > 256)
            || self.officer_titles.iter().flatten().any(|s| s.len() > 256)
            || self.officers.len() > 4096
            || self.approved.len() > 1024
            || self.banned_characters.len() > 1024
            || self.chat_gags.len() > 4096
        {
            return Err(SaveCodecError::Invalid("allegiance metadata limits"));
        }
        let mut ids = std::collections::BTreeSet::new();
        if self.officers.iter().any(|(id, rank)| {
            *id == 0 || *id == self.monarch || !(1..=3).contains(rank) || !ids.insert(*id)
        }) {
            return Err(SaveCodecError::Invalid("allegiance officer"));
        }
        ids.clear();
        if self.approved.iter().any(|id| *id == 0 || !ids.insert(*id)) {
            return Err(SaveCodecError::Invalid("allegiance approved"));
        }
        let mut accounts = std::collections::BTreeSet::new();
        if self
            .banned_characters
            .iter()
            .any(|(id, name)| *id == 0 || name.len() > 100 || !accounts.insert(*id))
        {
            return Err(SaveCodecError::Invalid("allegiance bans"));
        }
        ids.clear();
        if self
            .chat_gags
            .iter()
            .any(|(id, _)| *id == 0 || !ids.insert(*id))
        {
            return Err(SaveCodecError::Invalid("allegiance gags"));
        }
        if let Some(p) = self.sanctuary {
            let norm: f32 = p.rotation.iter().map(|v| v * v).sum();
            if p.cell == 0
                || p.origin
                    .iter()
                    .chain(p.rotation.iter())
                    .any(|v| !v.is_finite())
                || (norm - 1.0).abs() > 0.001
            {
                return Err(SaveCodecError::Invalid("allegiance sanctuary"));
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(116, 1, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 116, 1, limits())?;
        value.validate()?;
        Ok(value)
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}
