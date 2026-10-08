//! Frozen native save schemas. These contain value snapshots, never live world
//! structs. Field additions require a new schema and an explicit migration.
use crate::{CodecError, CodecLimits};
use bace_content::{ContentLimits, WeenieV1};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntitySaveV1 {
    pub object_id: u32,
    pub template_revision: u64,
    pub mutation_revision: u64,
    /// Frozen V1 numeric properties preserve unknown IDs and absent/zero values.
    pub state: WeenieV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestSaveV1 {
    pub name: String,
    pub completions: u32,
    /// Authoritative server epoch seconds; never a client clock.
    pub last_completed: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSaveV1 {
    pub entity: EntitySaveV1,
    pub account_id: u64,
    pub name: String,
    pub metadata: CharacterMetadataV1,
    #[serde(deserialize_with = "bounded")]
    pub quests: Vec<QuestSaveV1>,
}

/// ACE Character fields that are not numeric world-object properties. Current
/// title remains PropertyInt.CharacterTitleId in the frozen entity state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterMetadataV1 {
    pub hair_texture: u32,
    pub default_hair_texture: u32,
    pub options1: u32,
    pub options2: u32,
    #[serde(deserialize_with = "bounded")]
    pub titles: Vec<u32>,
    #[serde(deserialize_with = "bounded")]
    pub spell_favorites: Vec<SpellFavoriteV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellFavoriteV1 {
    pub spell_id: u32,
    /// Source widths and one-based stored bar/index are preserved.
    pub bar: u32,
    pub position: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseSaveV1 {
    pub entity: EntitySaveV1,
    pub owner: Option<u32>,
    pub death_operation: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HouseAccessV1 {
    pub player_id: u32,
    pub permissions: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HouseSaveV1 {
    pub entity: EntitySaveV1,
    pub house_id: u32,
    pub owner_id: u32,
    pub purchased_at: i64,
    pub rent_period_start: i64,
    pub rent_due_at: i64,
    #[serde(deserialize_with = "bounded")]
    pub access: Vec<HouseAccessV1>,
}

#[derive(Debug, thiserror::Error)]
pub enum SaveCodecError {
    #[error(transparent)]
    Envelope(#[from] CodecError),
    #[error(transparent)]
    Content(#[from] bace_content::ContentError),
    #[error("invalid frozen save: {0}")]
    Invalid(&'static str),
}

impl EntitySaveV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.object_id == 0 || self.object_id == u32::MAX || self.template_revision == 0 {
            return Err(SaveCodecError::Invalid(
                "entity identity or template revision",
            ));
        }
        self.state.validate(ContentLimits {
            max_entries: 16_384,
            max_string_bytes: 65_536,
        })?;
        Ok(())
    }
    pub fn encode_item(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(101, 1, self, limits())?)
    }
    pub fn decode_item(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 101, 1, limits())?;
        value.validate()?;
        Ok(value)
    }
}
impl PlayerSaveV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.entity.validate()?;
        if !(0x5000_0001..=0x5fff_ffff).contains(&self.entity.object_id)
            || self.account_id == 0
            || self.account_id > i64::MAX as u64
            || self.name.trim().is_empty()
            || self.name.len() > 100
            || self.name.chars().any(char::is_control)
            || self.quests.len() > 4096
        {
            return Err(SaveCodecError::Invalid(
                "player identity, name or quest count",
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        if self.metadata.titles.len() > 4096 || self.metadata.spell_favorites.len() > 4096 {
            return Err(SaveCodecError::Invalid("character metadata count"));
        }
        let mut titles = std::collections::BTreeSet::new();
        if self.metadata.titles.iter().any(|id| !titles.insert(*id)) {
            return Err(SaveCodecError::Invalid("duplicate character title"));
        }
        let mut favorites = std::collections::BTreeSet::new();
        for favorite in &self.metadata.spell_favorites {
            if favorite.spell_id == 0
                || favorite.bar == 0
                || favorite.position == 0
                || !favorites.insert((favorite.bar, favorite.position))
            {
                return Err(SaveCodecError::Invalid("invalid spell favorite"));
            }
        }
        for quest in &self.quests {
            if quest.name.is_empty()
                || quest.name.len() > 255
                || quest.last_completed < 0
                || !names.insert(&quest.name)
            {
                return Err(SaveCodecError::Invalid(
                    "quest identity, timestamp or duplicate",
                ));
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(100, 1, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 100, 1, limits())?;
        value.validate()?;
        Ok(value)
    }
}
impl CorpseSaveV1 {
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(102, 1, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 102, 1, limits())?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.entity.validate()?;
        if self.owner == Some(0)
            || self.death_operation.is_empty()
            || self.death_operation.len() > 128
            || self.expires_at < 0
        {
            return Err(SaveCodecError::Invalid("corpse owner, operation or expiry"));
        }
        Ok(())
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}

impl HouseSaveV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.entity.validate()?;
        if self.house_id == 0
            || !(0x5000_0001..=0x5fff_ffff).contains(&self.owner_id)
            || self.entity.object_id == self.owner_id
            || self.purchased_at < 0
            || self.rent_period_start < 0
            || self.rent_due_at < self.rent_period_start
            || self.access.len() > 4096
        {
            return Err(SaveCodecError::Invalid(
                "housing identity, rent or access count",
            ));
        }
        let mut ids = std::collections::BTreeSet::new();
        for access in &self.access {
            if !(0x5000_0001..=0x5fff_ffff).contains(&access.player_id)
                || !ids.insert(access.player_id)
            {
                return Err(SaveCodecError::Invalid(
                    "housing access identity or duplicate",
                ));
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(103, 1, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 103, 1, limits())?;
        value.validate()?;
        Ok(value)
    }
}

pub(crate) fn bounded<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    use serde::de::{Error, SeqAccess, Visitor};
    struct Entries<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Entries<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 4096 save entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > 4096) {
                return Err(A::Error::custom("quest limit"));
            }
            let mut result = Vec::new();
            while let Some(quest) = seq.next_element()? {
                if result.len() == 4096 {
                    return Err(A::Error::custom("quest limit"));
                }
                result.push(quest);
            }
            Ok(result)
        }
    }
    deserializer.deserialize_seq(Entries(std::marker::PhantomData))
}
