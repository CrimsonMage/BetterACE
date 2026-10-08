//! Frozen character UI supplement. Existing option masks/favorites stay in V1.
use crate::SaveCodecError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortcutSaveV1 {
    pub index: u32,
    pub object_id: u32,
    pub spell_id: u16,
    pub layer: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentPreferenceV1 {
    pub template_id: u32,
    pub quantity: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterUiV1 {
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub shortcuts: Vec<ShortcutSaveV1>,
    pub spellbook_filters: u32,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub desired_components: Vec<ComponentPreferenceV1>,
    #[serde(deserialize_with = "bounded_bytes")]
    pub gameplay_options: Vec<u8>,
}
impl Default for CharacterUiV1 {
    fn default() -> Self {
        Self {
            shortcuts: Vec::new(),
            spellbook_filters: 0x3fff,
            desired_components: Vec::new(),
            gameplay_options: Vec::new(),
        }
    }
}
impl CharacterUiV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.shortcuts.len() > 18
            || self.desired_components.len() > 4096
            || self.gameplay_options.len() > 65_536
        {
            return Err(SaveCodecError::Invalid("UI count/byte limit"));
        }
        let mut indexes = std::collections::BTreeSet::new();
        for shortcut in &self.shortcuts {
            if shortcut.index >= 18 || !indexes.insert(shortcut.index) {
                return Err(SaveCodecError::Invalid("shortcut index"));
            }
        }
        indexes.clear();
        for component in &self.desired_components {
            if component.template_id == 0
                || component.quantity == 0
                || component.quantity > i32::MAX as u32
                || !indexes.insert(component.template_id)
            {
                return Err(SaveCodecError::Invalid("component preference"));
            }
        }
        Ok(())
    }
}
fn bounded_bytes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    use serde::de::{Error, SeqAccess, Visitor};
    struct Bytes;
    impl<'de> Visitor<'de> for Bytes {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 65536 UI bytes")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            if seq.size_hint().is_some_and(|n| n > 65_536) {
                return Err(A::Error::custom("UI byte limit"));
            }
            let mut result = Vec::new();
            while let Some(byte) = seq.next_element()? {
                if result.len() == 65_536 {
                    return Err(A::Error::custom("UI byte limit"));
                }
                result.push(byte);
            }
            Ok(result)
        }
    }
    d.deserialize_seq(Bytes)
}
