//! Frozen detached NPC script source metadata. No physical body is restored from
//! these descriptive coordinates; live-world admission remains a separate gate.
use crate::{
    CodecLimits, NpcWorkflowSaveV1, NpcWorkflowSaveV2, SaveCodecError,
    npc_values_v1::{NpcPropertyFamilyV1 as F, NpcValueV1 as V},
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcArchivedPropertyV3 {
    pub family: F,
    pub stat: u32,
    pub value: V,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcSourceArchiveV3 {
    pub source: u32,
    pub player: bool,
    pub creature: bool,
    pub cell: u32,
    pub position: [f32; 3],
    pub heading: f32,
    pub property_revision: u64,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub properties: Vec<NpcArchivedPropertyV3>,
}
impl NpcSourceArchiveV3 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        let invalid = || SaveCodecError::Invalid("NPC detached source archive");
        if self.source == 0
            || self.cell == 0
            || self.player && !self.creature
            || self.position.iter().any(|v| !v.is_finite())
            || !self.heading.is_finite()
            || self.properties.len() > 4096
        {
            return Err(invalid());
        }
        validate_properties(&self.properties)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcLivePropertiesV3 {
    pub revision: u64,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub properties: Vec<NpcArchivedPropertyV3>,
}
impl NpcLivePropertiesV3 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        validate_properties(&self.properties)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcQuestEntryV3 {
    pub name: String,
    pub last_completed_seconds: u32,
    pub completions: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcSourceQuestsV3 {
    pub revision: u64,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub entries: Vec<NpcQuestEntryV3>,
}
impl NpcSourceQuestsV3 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        let invalid = || SaveCodecError::Invalid("NPC source quest snapshot");
        if self.entries.len() > 4096 {
            return Err(invalid());
        }
        let mut names = std::collections::BTreeSet::new();
        for row in &self.entries {
            if row.name.is_empty()
                || row.name.len() > 256
                || row.name.contains('@')
                || !names.insert(&row.name)
            {
                return Err(invalid());
            }
            let key: String = row
                .name
                .chars()
                .map(|c| {
                    let mut upper = c.to_uppercase();
                    let first = upper.next().unwrap_or(c);
                    if upper.next().is_none() { first } else { c }
                })
                .collect();
            if key != row.name {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
fn validate_properties(properties: &[NpcArchivedPropertyV3]) -> Result<(), SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC source property snapshot");
    if properties.len() > 4096 {
        return Err(invalid());
    }
    let mut keys = std::collections::BTreeSet::new();
    let mut bytes = 128usize;
    for p in properties {
        let tag = match p.family {
            F::Bool => 0,
            F::Int => 1,
            F::Int64 => 2,
            F::Float => 3,
            F::String => 4,
            F::Attribute => 5,
            F::RawAttribute => 6,
            F::Vital => 7,
            F::RawVital => 8,
            F::Skill => 9,
            F::RawSkill => 10,
            F::SkillAdvancement => 11,
        };
        if p.stat > 65535 || !keys.insert((tag, p.stat)) {
            return Err(invalid());
        }
        let valid = match (&p.family, &p.value) {
            (F::Bool, V::Bool(_)) | (F::Int, V::Int(_)) | (F::Int64, V::Int64(_)) => true,
            (F::Float, V::Float(v)) => v.is_finite(),
            (F::String, V::String(v)) => v.len() <= 65536,
            (
                F::Attribute
                | F::RawAttribute
                | F::Vital
                | F::RawVital
                | F::Skill
                | F::RawSkill
                | F::SkillAdvancement,
                V::Unsigned(_),
            ) => true,
            _ => false,
        };
        if !valid {
            return Err(invalid());
        }
        bytes = bytes
            .checked_add(match &p.value {
                V::String(s) => s.len().checked_add(32).ok_or_else(invalid)?,
                _ => 32,
            })
            .ok_or_else(invalid)?;
    }
    if bytes > 2 * 1024 * 1024 {
        return Err(invalid());
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcSourceLocationV3 {
    pub cell: u32,
    pub position: [f32; 3],
    pub heading: f32,
    pub player: bool,
    pub creature: bool,
}
impl NpcSourceLocationV3 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.cell == 0
            || self.player && !self.creature
            || self.position.iter().any(|v| !v.is_finite())
            || !self.heading.is_finite()
        {
            return Err(SaveCodecError::Invalid("NPC live source location"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcSourceInventoryItemV3 {
    pub id: u32,
    pub revision: u64,
    pub persisted_version: i64,
    pub registry_revision: Option<u64>,
    pub container: u32,
    pub slot: u32,
    pub pack_slot: bool,
    pub equipped: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcGeneratorOriginV3 {
    pub generator: u32,
    pub incarnation: u64,
    pub content_revision: u64,
    pub profile: u32,
    pub child_incarnation: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcSourceInventoryV3 {
    pub source_persisted_version: i64,
    pub source_mutation_revision: u64,
    pub origin: Option<NpcGeneratorOriginV3>,
    pub ticket: u64,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub items: Vec<NpcSourceInventoryItemV3>,
    pub source_registry_revision: Option<u64>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub source_enchantments: Vec<crate::FrozenEnchantmentV1>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub death_items: Vec<u32>,
}
impl NpcSourceInventoryV3 {
    pub fn validate(&self, source: u32) -> Result<(), SaveCodecError> {
        let invalid = || SaveCodecError::Invalid("NPC source inventory fence");
        if self.source_persisted_version < 0
            || self.ticket == 0
            || self.items.len() > 1024
            || self.death_items.len() > 1024
            || self.source_registry_revision.is_none() && !self.source_enchantments.is_empty()
        {
            return Err(invalid());
        }
        if self.origin.as_ref().is_some_and(|o| {
            o.generator == 0
                || o.incarnation == 0
                || o.content_revision == 0
                || o.child_incarnation == 0
        }) {
            return Err(invalid());
        }
        crate::validate_enchantments_v1(&self.source_enchantments)?;
        let mut indexed = std::collections::BTreeMap::new();
        for item in &self.items {
            if item.id == 0
                || item.id == source
                || item.container == 0
                || item.revision == 0
                || item.persisted_version < 0
                || indexed.insert(item.id, item).is_some()
            {
                return Err(invalid());
            }
        }
        let mut drop_ids = std::collections::BTreeSet::new();
        if self
            .death_items
            .iter()
            .any(|id| !indexed.contains_key(id) || !drop_ids.insert(*id))
        {
            return Err(invalid());
        }
        for item in &self.items {
            let mut at = item.container;
            let mut seen = std::collections::BTreeSet::from([item.id]);
            while at != source {
                if !seen.insert(at) || seen.len() > 64 {
                    return Err(invalid());
                }
                at = indexed.get(&at).ok_or_else(invalid)?.container;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcWorkflowSaveV3 {
    pub inventory: Option<NpcSourceInventoryV3>,
    pub location: Option<NpcSourceLocationV3>,
    pub live_properties: Option<NpcLivePropertiesV3>,
    pub source_quests: Option<NpcSourceQuestsV3>,
    pub previous: NpcWorkflowSaveV2,
    pub source_version: u64,
    pub source_template: u32,
    pub archive: Option<NpcSourceArchiveV3>,
}
impl std::ops::Deref for NpcWorkflowSaveV3 {
    type Target = NpcWorkflowSaveV2;
    fn deref(&self) -> &Self::Target {
        &self.previous
    }
}
impl From<NpcWorkflowSaveV2> for NpcWorkflowSaveV3 {
    fn from(previous: NpcWorkflowSaveV2) -> Self {
        Self {
            inventory: None,
            location: None,
            previous,
            source_version: 0,
            source_template: 0,
            archive: None,
            live_properties: None,
            source_quests: None,
        }
    }
}
impl From<NpcWorkflowSaveV1> for NpcWorkflowSaveV3 {
    fn from(previous: NpcWorkflowSaveV1) -> Self {
        Self::from(NpcWorkflowSaveV2::from(previous))
    }
}
impl NpcWorkflowSaveV3 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        self.previous.validate()?;
        if let Some(inventory) = &self.inventory {
            inventory.validate(self.source)?;
            if self.archive.is_some() {
                return Err(SaveCodecError::Invalid("NPC archived source inventory"));
            }
        }

        if let Some(location) = &self.location {
            location.validate()?;
            if self.archive.is_some() {
                return Err(SaveCodecError::Invalid(
                    "NPC source live/archive location conflict",
                ));
            }
        }

        if self.archive.is_some() && self.live_properties.is_some() {
            return Err(SaveCodecError::Invalid(
                "NPC source cannot be live and archived",
            ));
        }
        if let Some(properties) = &self.live_properties {
            properties.validate()?;
        }
        if let Some(quests) = &self.source_quests {
            quests.validate()?;
        }
        if self.source_version > i64::MAX as u64
            || self.source_version == 0 && self.archive.is_some()
            || self.source_version > 0 && self.source_template == 0
        {
            return Err(SaveCodecError::Invalid("NPC source head version/template"));
        }
        if let Some(archive) = &self.archive {
            archive.validate()?;
            if archive.source != self.previous.source {
                return Err(SaveCodecError::Invalid("NPC archive source identity"));
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        if self.source_version == 0 {
            return Err(SaveCodecError::Invalid("NPC source head version"));
        }
        Ok(crate::encode(120, 3, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 120, 3, limits())?;
        value.validate()?;
        if value.source_version == 0 {
            return Err(SaveCodecError::Invalid("NPC source head version"));
        }
        Ok(value)
    }
    pub fn decode_or_migrate(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let header = crate::inspect(bytes, limits())?;
        if header.kind != 120 {
            return Err(SaveCodecError::Invalid("NPC workflow kind"));
        }
        match header.schema_version {
            1 | 2 => {
                let value = Self::from(NpcWorkflowSaveV2::decode_or_migrate(bytes)?);
                value.validate()?;
                Ok(value)
            }
            3 => Self::decode(bytes),
            _ => Err(SaveCodecError::Invalid("unsupported NPC workflow schema")),
        }
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 4 * 1024 * 1024,
    }
}
