//! Frozen corpse access supplement. The viewer is deliberately transient; the
//! right to reopen after consuming a one-shot permit and the public-loot state
//! survive restart with the corpse's exact death identity and deadline.
use crate::{CodecLimits, CorpseSaveV4, ItemPlacementV2, SaveCodecError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseAccessSaveV1 {
    pub victim: Option<u32>,
    pub killer: Option<u32>,
    pub is_monster: bool,
    pub generated_rare: bool,
    pub pk_death: bool,
    pub looted: bool,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub permittees: Vec<u32>,
}
impl CorpseAccessSaveV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.victim == Some(0)
            || self.killer == Some(0)
            || self.permittees.len() > 1024
            || self.permittees.contains(&0)
            || self.permittees.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(SaveCodecError::Invalid("corpse access rights"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseSaveV5 {
    pub previous: CorpseSaveV4,
    pub access: CorpseAccessSaveV1,
}
impl std::ops::Deref for CorpseSaveV5 {
    type Target = CorpseSaveV4;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for CorpseSaveV5 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl CorpseSaveV5 {
    pub fn migrate_v4(previous: CorpseSaveV4) -> Result<Self, SaveCodecError> {
        let victim = previous.source;
        let killer = previous
            .corpse
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .find(|p| p.id == 19)
            .map(|p| p.value)
            .filter(|id| *id != 0);
        let generated_rare = previous
            .corpse
            .entity
            .state
            .properties
            .bools
            .iter()
            .any(|p| p.id == 102 && p.value);
        let pk_death = previous
            .corpse
            .entity
            .state
            .properties
            .ints
            .iter()
            .any(|p| p.id == 99 && p.value == 1);
        let value = Self {
            access: CorpseAccessSaveV1 {
                victim,
                killer,
                is_monster: victim.is_some_and(|id| !(0x5000_0001..0x6000_0000).contains(&id)),
                generated_rare,
                pk_death,
                looted: false,
                permittees: Vec::new(),
            },
            previous,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        self.access.validate()?;
        if self.previous.source != self.access.victim
            || self.access.victim.is_some_and(|id| {
                self.access.is_monster == (0x5000_0001..0x6000_0000).contains(&id)
            })
        {
            return Err(SaveCodecError::Invalid("corpse access victim identity"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(102, 5, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 102, 5, limits())?;
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
            1..=4 => Self::migrate_v4(CorpseSaveV4::decode_or_migrate(bytes, placement)?),
            5 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported save schema")),
        }
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}

/// Static rights never change. A confirmed close may publish IsLooted, and a
/// confirmed one-shot permit may add exactly one permittee at a newer revision.
pub fn validate_corpse_transition_v5(
    before: &CorpseSaveV5,
    after: &CorpseSaveV5,
) -> Result<(), SaveCodecError> {
    crate::validate_corpse_transition(&before.previous, &after.previous)?;
    before.validate()?;
    after.validate()?;
    let source_enrichment = before.previous.source.is_none()
        && before.previous.operation.is_none()
        && after.previous.source.is_some()
        && before.access.victim.is_none()
        && !before.access.is_monster
        && !before.access.looted
        && before.access.permittees.is_empty()
        && after.corpse.entity.mutation_revision > before.corpse.entity.mutation_revision;
    if (!source_enrichment
        && (before.access.victim != after.access.victim
            || before.access.is_monster != after.access.is_monster))
        || before.access.killer != after.access.killer
        || before.access.generated_rare != after.access.generated_rare
        || before.access.pk_death != after.access.pk_death
        || before.access.looted && !after.access.looted
        || before
            .access
            .permittees
            .iter()
            .any(|id| after.access.permittees.binary_search(id).is_err())
        || (before.access.looted != after.access.looted
            || before.access.permittees != after.access.permittees)
            && after.corpse.entity.mutation_revision <= before.corpse.entity.mutation_revision
    {
        return Err(SaveCodecError::Invalid("corpse access transition"));
    }
    Ok(())
}
