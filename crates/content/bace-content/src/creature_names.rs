//! Frozen derived name index keyed by accepted template identity. It permits
//! startup name checks without decoding every weenie in the runtime world pack.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureNameIndexV1 {
    pub schema_version: u16,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub entries: Vec<CreatureNameV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureNameV1 {
    pub template: u32,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateClassIndexV1 {
    pub schema_version: u16,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub entries: Vec<TemplateClassIdentityV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateClassIdentityV1 {
    pub template: u32,
    pub class_name: String,
}
impl TemplateClassIndexV1 {
    pub fn validate(&self) -> Result<(), String> {
        let mut names = std::collections::BTreeSet::new();
        if self.schema_version != 1
            || self.entries.len() > 100_000
            || self
                .entries
                .windows(2)
                .any(|rows| rows[0].template >= rows[1].template)
            || self.entries.iter().any(|row| {
                row.template == 0
                    || row.class_name.is_empty()
                    || row.class_name.len() > 255
                    || !names.insert(&row.class_name)
            })
            || self
                .entries
                .iter()
                .try_fold(0usize, |n, row| n.checked_add(row.class_name.len()))
                .is_none_or(|n| n > 8 * 1024 * 1024)
        {
            return Err("invalid template class-name index".into());
        }
        Ok(())
    }
}
impl CreatureNameIndexV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.entries.len() > 100_000
            || self
                .entries
                .windows(2)
                .any(|rows| rows[0].template >= rows[1].template)
            || self
                .entries
                .iter()
                .any(|row| row.template == 0 || row.name.len() > 1024)
            || self
                .entries
                .iter()
                .try_fold(0usize, |n, row| n.checked_add(row.name.len()))
                .is_none_or(|n| n > 8 * 1024 * 1024)
        {
            return Err("invalid creature name index".into());
        }
        Ok(())
    }
}
