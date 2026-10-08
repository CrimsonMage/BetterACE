//! Frozen schema 3 preserves schema 2 and adds UI/registry supplements.
use crate::{
    CharacterUiV1, CodecLimits, CorpseSaveV2, FrozenEnchantmentV1, HouseSaveV2, ItemPlacementV2,
    ItemSaveV2, PlayerSaveV2, SaveCodecError,
};
use serde::{Deserialize, Serialize};

fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSaveV3 {
    pub previous: PlayerSaveV2,
    pub ui: CharacterUiV1,
}
impl std::ops::Deref for PlayerSaveV3 {
    type Target = PlayerSaveV2;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for PlayerSaveV3 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl PlayerSaveV3 {
    pub fn migrate_v1(previous: crate::PlayerSaveV1) -> Result<Self, SaveCodecError> {
        Self::migrate_v2(PlayerSaveV2::migrate_v1(previous)?)
    }
    pub fn migrate_v2(previous: PlayerSaveV2) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            ui: Default::default(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        self.ui.validate()
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(100, 3, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 100, 3, limits())?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_or_migrate(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let info = crate::inspect(bytes, limits())?;
        if info.kind != 100 {
            return Err(SaveCodecError::Invalid("save kind"));
        }
        match info.schema_version {
            1 | 2 => Self::migrate_v2(PlayerSaveV2::decode_or_migrate(bytes)?),
            3 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported save schema")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSaveV3 {
    pub previous: ItemSaveV2,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub enchantments: Vec<FrozenEnchantmentV1>,
}
impl std::ops::Deref for ItemSaveV3 {
    type Target = ItemSaveV2;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for ItemSaveV3 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl ItemSaveV3 {
    pub fn migrate_v2(previous: ItemSaveV2) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            enchantments: Default::default(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        crate::validate_enchantments_v1(&self.enchantments)
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(101, 3, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 101, 3, limits())?;
        value.validate()?;
        Ok(value)
    }
    /// Schema 1 requires authoritative relational placement; never guess removal.
    pub fn decode_or_migrate(
        bytes: &[u8],
        placement: Option<ItemPlacementV2>,
    ) -> Result<Self, SaveCodecError> {
        let info = crate::inspect(bytes, limits())?;
        if info.kind != 101 {
            return Err(SaveCodecError::Invalid("save kind"));
        }
        match info.schema_version {
            1 => Self::migrate_v2(ItemSaveV2::migrate_v1(
                crate::EntitySaveV1::decode_item(bytes)?,
                placement.ok_or(SaveCodecError::Invalid("V1 placement required"))?,
            )?),
            2 => Self::migrate_v2(ItemSaveV2::decode(bytes)?),
            3 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported save schema")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseSaveV3 {
    pub previous: CorpseSaveV2,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub enchantments: Vec<FrozenEnchantmentV1>,
}
impl std::ops::Deref for CorpseSaveV3 {
    type Target = CorpseSaveV2;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for CorpseSaveV3 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl CorpseSaveV3 {
    pub fn migrate_v2(previous: CorpseSaveV2) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            enchantments: Default::default(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        crate::validate_enchantments_v1(&self.enchantments)
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(102, 3, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 102, 3, limits())?;
        value.validate()?;
        Ok(value)
    }
    /// Schema 1 requires authoritative relational placement; never guess removal.
    pub fn decode_or_migrate(
        bytes: &[u8],
        placement: Option<ItemPlacementV2>,
    ) -> Result<Self, SaveCodecError> {
        let info = crate::inspect(bytes, limits())?;
        if info.kind != 102 {
            return Err(SaveCodecError::Invalid("save kind"));
        }
        match info.schema_version {
            1 => Self::migrate_v2(CorpseSaveV2::migrate_v1(
                crate::CorpseSaveV1::decode(bytes)?,
                placement.ok_or(SaveCodecError::Invalid("V1 placement required"))?,
            )?),
            2 => Self::migrate_v2(CorpseSaveV2::decode(bytes)?),
            3 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported save schema")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HouseSaveV3 {
    pub previous: HouseSaveV2,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub enchantments: Vec<FrozenEnchantmentV1>,
}
impl std::ops::Deref for HouseSaveV3 {
    type Target = HouseSaveV2;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for HouseSaveV3 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl HouseSaveV3 {
    pub fn migrate_v1(previous: crate::HouseSaveV1) -> Result<Self, SaveCodecError> {
        Self::migrate_v2(HouseSaveV2::migrate_v1(previous)?)
    }
    pub fn migrate_v2(previous: HouseSaveV2) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            enchantments: Default::default(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        crate::validate_enchantments_v1(&self.enchantments)
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(103, 3, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 103, 3, limits())?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_migrate(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let info = crate::inspect(bytes, limits())?;
        if info.kind != 103 {
            return Err(SaveCodecError::Invalid("save kind"));
        }
        match info.schema_version {
            1 | 2 => Self::migrate_v2(HouseSaveV2::decode_migrate(bytes)?),
            3 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported save schema")),
        }
    }
}
