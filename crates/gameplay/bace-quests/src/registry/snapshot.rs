//! Bounded owner restoration preserves canonical names and the exact revision.
use super::*;
impl QuestRegistry {
    pub fn restore_snapshot(
        revision: u64,
        rows: Vec<(String, QuestProgress)>,
    ) -> Result<Self, QuestRegistryError> {
        if rows.len() > 4096 {
            return Err(QuestRegistryError::Capacity);
        }
        let mut entries = BTreeMap::new();
        for (name, progress) in rows {
            if quest_key(&name)? != name || entries.insert(name, progress).is_some() {
                return Err(QuestRegistryError::InvalidName);
            }
        }
        Ok(Self {
            entries,
            revision,
            capacity: 4096,
        })
    }
}
