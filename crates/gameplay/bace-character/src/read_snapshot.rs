//! Bounded immutable read snapshot. This is not a second mutable character owner
//! or a storage schema; persistence must still map into frozen versioned DTOs.
use crate::CharacterProgression;
#[derive(Debug)]
pub struct ProgressionSnapshot {
    state: CharacterProgression,
}
impl ProgressionSnapshot {
    pub fn state(&self) -> &CharacterProgression {
        &self.state
    }
}
impl CharacterProgression {
    /// Copies at most 256 trait entries. Immutable DAT/rank tables stay shared.
    /// No mutation or ownership-transfer API is exposed by the returned value.
    pub fn read_snapshot(&self) -> ProgressionSnapshot {
        ProgressionSnapshot {
            state: Self {
                traits: self.traits.clone(),
                tables: self.tables.clone(),
                available_experience: self.available_experience,
                revision: self.revision,
                training: self.training.clone(),
                luminance: self.luminance,
            },
        }
    }
}
