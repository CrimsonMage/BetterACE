//! Frozen schema1 for native character-start authoring. Sequence order is part
//! of the source contract; duplicate gear rows may intentionally combine stacks.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterStartProfileV1 {
    pub schema_version: u16,
    pub human_template: u32,
    pub default_start_spell: u32,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub gear: Vec<CharacterStartGearV1>,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub spells: Vec<CharacterStartSpellV1>,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub starts: Vec<CharacterStartAreaV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterStartGearV1 {
    pub skill: u32,
    pub heritage: Option<u32>,
    pub template: u32,
    pub count: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterStartSpellV1 {
    pub skill: u32,
    pub spell: u32,
    pub specialized_only: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterStartAreaV1 {
    pub name: String,
    /// None means the source explicitly uses the admitted starting location.
    pub free_ride_spell: Option<u32>,
}
impl CharacterStartProfileV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.human_template == 0
            || self.default_start_spell == 0
            || self.gear.len() > 4096
            || self.spells.len() > 4096
            || self.starts.len() > 256
            || self.gear.iter().any(|g| {
                g.skill >= 55
                    || g.heritage == Some(0)
                    || g.template == 0
                    || g.count == 0
                    || g.count > i32::MAX as u32
            })
            || self
                .spells
                .iter()
                .any(|s| s.skill >= 55 || s.spell == 0 || s.spell > 65535)
        {
            return Err("invalid character-start profile".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for start in &self.starts {
            if start.name.is_empty()
                || start.name.len() > 128
                || start.name.chars().any(char::is_control)
                || !names.insert(&start.name)
                || start.free_ride_spell == Some(0)
            {
                return Err("invalid character-start area".into());
            }
        }
        Ok(())
    }
}
