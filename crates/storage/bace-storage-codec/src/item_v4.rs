//! Frozen construction evidence for inventory-resident Creature/Cow instances.
//! The item aggregates remain authoritative for child state and placement.
use crate::{CodecLimits, ItemPlacementV2, ItemSaveV3, SaveCodecError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenGeneratorConstructionOriginV1 {
    pub generator: u32,
    pub incarnation: u64,
    pub content_revision: u64,
    pub profile: u32,
    pub occurrence: u64,
    pub random_identity: [u8; 16],
    pub random_key_version: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenConstructedChildV1 {
    pub entity: u32,
    /// None is directly contained by the companion root; Some names an earlier row.
    pub parent: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenCreatureConstructionV1 {
    pub weenie_type: u32,
    pub origin: FrozenGeneratorConstructionOriginV1,
    #[serde(deserialize_with = "bounded_members")]
    pub equipment_order: Vec<u32>,
    #[serde(deserialize_with = "bounded_members")]
    pub death_roster: Vec<FrozenConstructedChildV1>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSaveV4 {
    pub previous: ItemSaveV3,
    pub construction: Option<FrozenCreatureConstructionV1>,
}
impl std::ops::Deref for ItemSaveV4 {
    type Target = ItemSaveV3;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl std::ops::DerefMut for ItemSaveV4 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.previous
    }
}
impl ItemSaveV4 {
    pub fn migrate_v2(previous: crate::ItemSaveV2) -> Result<Self, SaveCodecError> {
        Self::migrate_v3(ItemSaveV3::migrate_v2(previous)?)
    }
    pub fn migrate_v3(previous: ItemSaveV3) -> Result<Self, SaveCodecError> {
        let value = Self {
            previous,
            construction: None,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        if let Some(construction) = &self.construction {
            construction.validate(self.entity.object_id, self.entity.state.weenie_type)?;
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(101, 4, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 101, 4, limits())?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_or_migrate(
        bytes: &[u8],
        placement: Option<ItemPlacementV2>,
    ) -> Result<Self, SaveCodecError> {
        let info = crate::inspect(bytes, limits())?;
        if info.kind != 101 {
            return Err(SaveCodecError::Invalid("save kind"));
        }
        match info.schema_version {
            1..=3 => Self::migrate_v3(ItemSaveV3::decode_or_migrate(bytes, placement)?),
            4 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported save schema")),
        }
    }
}
impl FrozenCreatureConstructionV1 {
    pub fn validate(&self, root: u32, weenie_type: u32) -> Result<(), SaveCodecError> {
        if root == 0
            || !matches!(self.weenie_type, 10 | 15)
            || self.weenie_type != weenie_type
            || self.origin.generator == 0
            || self.origin.incarnation == 0
            || self.origin.content_revision == 0
            || self.origin.random_key_version == 0
            || self.equipment_order.len() > 1024
            || self.death_roster.len() > 1024
        {
            return Err(SaveCodecError::Invalid(
                "creature construction identity/capacity",
            ));
        }
        let mut equipment = BTreeSet::new();
        if self
            .equipment_order
            .iter()
            .any(|&id| id == 0 || id == root || !equipment.insert(id))
        {
            return Err(SaveCodecError::Invalid("creature equipment order"));
        }
        let mut seen = BTreeSet::new();
        for row in &self.death_roster {
            if row.entity == 0
                || row.entity == root
                || row.parent.is_some_and(|id| !seen.contains(&id))
                || !seen.insert(row.entity)
            {
                return Err(SaveCodecError::Invalid("creature death roster"));
            }
        }
        Ok(())
    }
}
/// Construction origin/subtype cannot disappear during an ordinary item save.
/// Ordered membership may change only with a correspondingly validated graph;
/// that transactional graph check belongs to the owning persistence adapter.
pub fn validate_item_construction_transition(
    before: &ItemSaveV4,
    after: &ItemSaveV4,
) -> Result<(), SaveCodecError> {
    before.validate()?;
    after.validate()?;
    if before.entity.object_id != after.entity.object_id
        || before.construction.as_ref().is_some_and(|old| {
            after
                .construction
                .as_ref()
                .is_none_or(|new| old.weenie_type != new.weenie_type || old.origin != new.origin)
        })
    {
        return Err(SaveCodecError::Invalid(
            "creature construction identity changed",
        ));
    }
    Ok(())
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}

fn bounded_members<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    use serde::de::{Error, SeqAccess, Visitor};
    struct Members<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Members<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 1024 construction members")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|count| count > 1024) {
                return Err(A::Error::custom("construction member capacity"));
            }
            let mut result = Vec::new();
            while let Some(member) = seq.next_element()? {
                if result.len() == 1024 {
                    return Err(A::Error::custom("construction member capacity"));
                }
                result.push(member);
            }
            Ok(result)
        }
    }
    deserializer.deserialize_seq(Members(std::marker::PhantomData))
}
