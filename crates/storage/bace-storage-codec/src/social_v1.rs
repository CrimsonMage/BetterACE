//! Frozen social supplement. Preferences share the player's aggregate dirty revision.
use crate::SaveCodecError;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocialSquelchV1 {
    pub character: u32,
    pub account: Option<u64>,
    pub name: String,
    pub mask: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocialSaveV1 {
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub friends: Vec<u32>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub squelches: Vec<SocialSquelchV1>,
    pub global_mask: u32,
    pub afk_message: String,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub channels: Vec<u32>,
}
impl SocialSaveV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        let mut ids = std::collections::BTreeSet::new();
        if self.friends.len() > 1024
            || self.squelches.len() > 1024
            || self.channels.len() > 32
            || self.afk_message.len() > 4096
            || self.friends.iter().any(|id| *id == 0 || !ids.insert(*id))
        {
            return Err(SaveCodecError::Invalid("social limits or duplicate friend"));
        }
        ids.clear();
        if self.channels.iter().any(|id| !ids.insert(*id)) {
            return Err(SaveCodecError::Invalid("duplicate social channel"));
        }
        let mut squelches = std::collections::BTreeSet::new();
        for row in &self.squelches {
            if row.character == 0
                || row
                    .account
                    .is_some_and(|id| id == 0 || id > i64::MAX as u64)
                || row.name.len() > 100
                || !squelches.insert((
                    row.account.is_some(),
                    row.account.unwrap_or(u64::from(row.character)),
                ))
            {
                return Err(SaveCodecError::Invalid("social squelch identity"));
            }
        }
        Ok(())
    }
}
