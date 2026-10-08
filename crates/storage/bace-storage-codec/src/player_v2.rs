use crate::{CodecLimits, FrozenEnchantmentV1, PlayerSaveV1, RareStateV1, SaveCodecError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSaveV2 {
    /// V1 remains frozen, including its exact field ordering.
    pub player: PlayerSaveV1,
    pub rares: Option<RareStateV1>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub enchantments: Vec<FrozenEnchantmentV1>,
}
impl PlayerSaveV2 {
    /// Explicit native V1 migration. No seed, timer or enchantment is invented.
    pub fn migrate_v1(player: PlayerSaveV1) -> Result<Self, SaveCodecError> {
        player.validate()?;
        Ok(Self {
            player,
            rares: None,
            enchantments: Vec::new(),
        })
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.player.validate()?;
        if let Some(state) = &self.rares {
            state.validate(self.player.entity.object_id)?;
        }
        crate::validate_enchantments_v1(&self.enchantments)
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(100, 2, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 100, 2, limits())?;
        value.validate()?;
        Ok(value)
    }
    /// Load a known schema, explicitly migrating native V1 in memory. The owning
    /// persistence adapter must CAS-save V2; this does not acknowledge migration.
    pub fn decode_or_migrate(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let info = crate::inspect(bytes, limits())?;
        if info.kind != 100 {
            return Err(crate::CodecError::Kind {
                expected: 100,
                actual: info.kind,
            }
            .into());
        }
        match info.schema_version {
            1 => Self::migrate_v1(PlayerSaveV1::decode(bytes)?),
            2 => Self::decode(bytes),
            actual => Err(crate::CodecError::Schema {
                expected: 2,
                actual,
            }
            .into()),
        }
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}
