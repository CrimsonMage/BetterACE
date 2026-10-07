use std::collections::BTreeMap;
use std::sync::Arc;

use bace_gameplay_api::{
    ProgressionChange, ProgressionProjection, ProgressionRejection, ProgressionTarget,
    RaiseProgression, SkillAdvancement, TraitDetails,
};

use crate::{ProgressionTables, RankTable};

/// Input from validated creation/load state. Skill advancement is only used for
/// skill targets; non-skill targets must use Inactive to avoid ambiguous state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraitProgress {
    pub target: ProgressionTarget,
    pub experience_spent: u32,
    pub advancement: SkillAdvancement,
}

/// Complete authoritative trait input for packet-ready read projections. This
/// is not a persistence DTO; load adapters must explicitly map frozen schemas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraitState {
    pub progress: TraitProgress,
    pub details: TraitDetails,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OwnedTrait {
    pub(crate) progress: TraitProgress,
    pub(crate) details: Option<TraitDetails>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressionStateError {
    TooManyTraits,
    DuplicateTarget,
    InvalidAdvancement,
    ExperienceBeyondMaximum,
    ExperienceOnUntrainedSkill,
    AvailableExperienceOutOfRange,
    WrongTraitDetails,
    NonfiniteSkillTime,
}

/// Simulation-owned progression aggregate. No network, persistence, clocks or
/// random sources. It must not be copied into a second mutable world owner.
#[derive(Debug)]
pub struct CharacterProgression {
    pub(crate) traits: BTreeMap<ProgressionTarget, OwnedTrait>,
    pub(crate) tables: Arc<ProgressionTables>,
    available_experience: u64,
    pub(crate) revision: u64,
    pub(crate) training: Option<crate::training_rules::TrainingState>,
}

impl CharacterProgression {
    pub fn new(
        traits: &[TraitProgress],
        tables: Arc<ProgressionTables>,
        available_experience: u64,
        revision: u64,
    ) -> Result<Self, ProgressionStateError> {
        // More than all known skills plus attributes/vitals; bounds allocations
        // before inspecting externally loaded records.
        if traits.len() > 256 {
            return Err(ProgressionStateError::TooManyTraits);
        }
        if available_experience > i64::MAX as u64 {
            return Err(ProgressionStateError::AvailableExperienceOutOfRange);
        }
        let mut entries = BTreeMap::new();
        for entry in traits {
            match entry.target {
                ProgressionTarget::Skill(_) => {
                    if matches!(
                        entry.advancement,
                        SkillAdvancement::Inactive | SkillAdvancement::Untrained
                    ) && entry.experience_spent != 0
                    {
                        return Err(ProgressionStateError::ExperienceOnUntrainedSkill);
                    }
                }
                _ if entry.advancement != SkillAdvancement::Inactive => {
                    return Err(ProgressionStateError::InvalidAdvancement);
                }
                _ => {}
            }
            if let Some(table) = table_for(&tables, entry)
                && entry.experience_spent > table.maximum_experience()
            {
                return Err(ProgressionStateError::ExperienceBeyondMaximum);
            }
            if entries
                .insert(
                    entry.target,
                    OwnedTrait {
                        progress: *entry,
                        details: None,
                    },
                )
                .is_some()
            {
                return Err(ProgressionStateError::DuplicateTarget);
            }
        }
        Ok(Self {
            traits: entries,
            tables,
            available_experience,
            revision,
            training: None,
        })
    }

    /// Supply actual prepared or loaded initial/vital/skill values. The legacy
    /// new() constructor remains useful for formula tests but has no metadata
    /// and therefore cannot support a complete replication trait update.
    pub fn with_state(
        states: &[TraitState],
        tables: Arc<ProgressionTables>,
        available_experience: u64,
        revision: u64,
    ) -> Result<Self, ProgressionStateError> {
        if states.len() > 256 {
            return Err(ProgressionStateError::TooManyTraits);
        }
        for state in states {
            match (state.progress.target, state.details) {
                (ProgressionTarget::Attribute(_), TraitDetails::Attribute { .. })
                | (ProgressionTarget::Vital(_), TraitDetails::Vital { .. }) => {}
                (ProgressionTarget::Skill(_), TraitDetails::Skill { last_used_time, .. }) => {
                    if !last_used_time.is_finite() {
                        return Err(ProgressionStateError::NonfiniteSkillTime);
                    }
                }
                _ => return Err(ProgressionStateError::WrongTraitDetails),
            }
        }
        let progress: Vec<_> = states.iter().map(|state| state.progress).collect();
        let mut result = Self::new(&progress, tables, available_experience, revision)?;
        for state in states {
            result
                .traits
                .get_mut(&state.progress.target)
                .expect("validated trait")
                .details = Some(state.details);
        }
        Ok(result)
    }

    pub fn available_experience(&self) -> u64 {
        self.available_experience
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Immutable records for a simulation-owned save adapter. The adapter must
    /// translate these into frozen versioned DTOs, never persist this type itself.
    pub fn traits(&self) -> impl ExactSizeIterator<Item = TraitProgress> + '_ {
        self.traits.values().map(|entry| entry.progress)
    }

    /// Complete immutable state for adapters; None explicitly denotes inputs
    /// built with the legacy math-only constructor, never invented defaults.
    pub fn trait_states(
        &self,
    ) -> impl ExactSizeIterator<Item = (TraitProgress, Option<TraitDetails>)> + '_ {
        self.traits
            .values()
            .map(|entry| (entry.progress, entry.details))
    }

    pub fn projection(&self, target: ProgressionTarget) -> Option<ProgressionProjection> {
        let entry = self.traits.get(&target)?;
        Some(ProgressionProjection {
            target,
            experience_spent: entry.progress.experience_spent,
            ranks: table_for(&self.tables, &entry.progress)
                .map_or(0, |table| table.rank(entry.progress.experience_spent)),
            advancement: entry.progress.advancement,
            details: entry.details,
        })
    }

    /// Ports ACE's RaiseAttribute/RaiseVital/RaiseSkill expenditure semantics.
    /// Failure leaves every field untouched. The caller owns authentication,
    /// action sequencing, derived-stat refresh, effects and save scheduling.
    pub fn raise(
        &mut self,
        request: RaiseProgression,
    ) -> Result<ProgressionChange, ProgressionRejection> {
        let entry = self
            .traits
            .get_mut(&request.target)
            .ok_or(ProgressionRejection::UnknownTarget)?;
        let details = entry.details;
        let entry = &mut entry.progress;
        let table = table_for(&self.tables, entry).ok_or(ProgressionRejection::UntrainedSkill)?;
        if u64::from(request.amount) > self.available_experience {
            return Err(ProgressionRejection::InsufficientExperience);
        }
        let before = ProgressionProjection {
            target: request.target,
            experience_spent: entry.experience_spent,
            ranks: table.rank(entry.experience_spent),
            advancement: entry.advancement,
            details,
        };
        if before.ranks == table.maximum_rank() {
            return Err(ProgressionRejection::MaximumRank);
        }
        if request.amount > table.maximum_experience() - entry.experience_spent {
            return Err(ProgressionRejection::ExceedsMaximumExperience);
        }
        let revision = if request.amount == 0 {
            self.revision
        } else {
            self.revision
                .checked_add(1)
                .ok_or(ProgressionRejection::RevisionExhausted)?
        };
        let experience_spent = entry.experience_spent + request.amount;
        let after = ProgressionProjection {
            target: request.target,
            experience_spent,
            ranks: table.rank(experience_spent),
            advancement: entry.advancement,
            details,
        };
        entry.experience_spent = experience_spent;
        self.available_experience -= u64::from(request.amount);
        self.revision = revision;
        Ok(ProgressionChange {
            before,
            after,
            available_experience: self.available_experience,
            revision,
        })
    }
}

fn table_for<'a>(tables: &'a ProgressionTables, entry: &TraitProgress) -> Option<&'a RankTable> {
    match entry.target {
        ProgressionTarget::Attribute(_) => Some(&tables.attributes),
        ProgressionTarget::Vital(_) => Some(&tables.vitals),
        ProgressionTarget::Skill(_) => match entry.advancement {
            SkillAdvancement::Trained => Some(&tables.trained_skills),
            SkillAdvancement::Specialized => Some(&tables.specialized_skills),
            SkillAdvancement::Inactive | SkillAdvancement::Untrained => None,
        },
    }
}
