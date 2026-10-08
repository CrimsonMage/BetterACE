//! Pinned ACE AttributeTransferDevice transfer of innate starting values.
//! The simulation owner must commit this proposal with consumption of one device.
use crate::CharacterProgression;
use bace_gameplay_api::{AttributeId, ProgressionProjection, ProgressionTarget, TraitDetails};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeTransferError {
    MissingAttribute,
    MissingStartingValue,
    WieldRequirement,
    SourceAtMinimum,
    TargetAtMaximum,
    RevisionExhausted,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttributeTransferProposal {
    pub from_before: ProgressionProjection,
    pub from_after: ProgressionProjection,
    pub to_before: ProgressionProjection,
    pub to_after: ProgressionProjection,
    pub amount: u32,
    pub expected_revision: u64,
    pub revision: u64,
}

impl CharacterProgression {
    /// Source checks any equipped RawAttrib/Attrib requirement before checking
    /// either bound, even when that particular item would remain wieldable.
    pub fn propose_attribute_transfer(
        &self,
        from: AttributeId,
        to: AttributeId,
        equipped_attribute_requirement: bool,
    ) -> Result<AttributeTransferProposal, AttributeTransferError> {
        if equipped_attribute_requirement {
            return Err(AttributeTransferError::WieldRequirement);
        }
        let from_before = self
            .projection(ProgressionTarget::Attribute(from))
            .ok_or(AttributeTransferError::MissingAttribute)?;
        let to_before = self
            .projection(ProgressionTarget::Attribute(to))
            .ok_or(AttributeTransferError::MissingAttribute)?;
        let Some(TraitDetails::Attribute {
            starting_value: from_start,
        }) = from_before.details
        else {
            return Err(AttributeTransferError::MissingStartingValue);
        };
        let Some(TraitDetails::Attribute {
            starting_value: to_start,
        }) = to_before.details
        else {
            return Err(AttributeTransferError::MissingStartingValue);
        };
        if from_start <= 10 {
            return Err(AttributeTransferError::SourceAtMinimum);
        }
        if to_start >= 100 {
            return Err(AttributeTransferError::TargetAtMaximum);
        }
        let amount = 10.min(from_start - 10).min(100 - to_start);
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(AttributeTransferError::RevisionExhausted)?;
        let mut from_after = from_before;
        let mut to_after = to_before;
        // ACE applies both mutations to the same attribute object when content
        // names the same source and destination, producing no net value change.
        if from == to {
            from_after.details = from_before.details;
            to_after.details = to_before.details;
        } else {
            from_after.details = Some(TraitDetails::Attribute {
                starting_value: from_start - amount,
            });
            to_after.details = Some(TraitDetails::Attribute {
                starting_value: to_start + amount,
            });
        }
        Ok(AttributeTransferProposal {
            from_before,
            from_after,
            to_before,
            to_after,
            amount,
            expected_revision: self.revision,
            revision,
        })
    }

    /// Called only after an exact combined character+item durable receipt.
    pub fn adopt_attribute_transfer(
        &mut self,
        proposal: AttributeTransferProposal,
    ) -> Result<(), AttributeTransferError> {
        if self.revision != proposal.expected_revision
            || self.projection(proposal.from_before.target) != Some(proposal.from_before)
            || self.projection(proposal.to_before.target) != Some(proposal.to_before)
            || self.revision.checked_add(1) != Some(proposal.revision)
        {
            return Err(AttributeTransferError::Stale);
        }
        let from = self
            .traits
            .get_mut(&proposal.from_before.target)
            .ok_or(AttributeTransferError::Stale)?;
        from.details = proposal.from_after.details;
        let to = self
            .traits
            .get_mut(&proposal.to_before.target)
            .ok_or(AttributeTransferError::Stale)?;
        to.details = proposal.to_after.details;
        self.revision = proposal.revision;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProgressionTables, RankTable, TraitProgress, TraitState};
    use bace_gameplay_api::SkillAdvancement;
    use std::sync::Arc;

    fn character(from: u32, to: u32) -> CharacterProgression {
        let states = [
            TraitState {
                progress: TraitProgress {
                    target: ProgressionTarget::Attribute(AttributeId::Strength),
                    experience_spent: 0,
                    advancement: SkillAdvancement::Inactive,
                },
                details: TraitDetails::Attribute {
                    starting_value: from,
                },
            },
            TraitState {
                progress: TraitProgress {
                    target: ProgressionTarget::Attribute(AttributeId::Endurance),
                    experience_spent: 0,
                    advancement: SkillAdvancement::Inactive,
                },
                details: TraitDetails::Attribute { starting_value: to },
            },
        ];
        let table = RankTable::new(&[0, 100]).unwrap();
        CharacterProgression::with_state(
            &states,
            Arc::new(ProgressionTables {
                attributes: table.clone(),
                vitals: table.clone(),
                trained_skills: table.clone(),
                specialized_skills: table,
            }),
            0,
            7,
        )
        .unwrap()
    }

    #[test]
    fn pinned_transfer_bounds_and_exact_adoption() {
        // AttributeTransferDevice.ActOnUse takes the smallest of 10, source
        // surplus above 10, and destination room below 100.
        for (from, to, amount) in [(50, 50, 10), (13, 50, 3), (50, 97, 3)] {
            let mut character = character(from, to);
            let proposed = character
                .propose_attribute_transfer(AttributeId::Strength, AttributeId::Endurance, false)
                .unwrap();
            assert_eq!(proposed.amount, amount);
            assert_eq!(character.revision(), 7);
            character.adopt_attribute_transfer(proposed).unwrap();
            assert_eq!(character.revision(), 8);
            assert_eq!(
                character
                    .projection(ProgressionTarget::Attribute(AttributeId::Strength))
                    .unwrap()
                    .details,
                Some(TraitDetails::Attribute {
                    starting_value: from - amount
                })
            );
            assert_eq!(
                character
                    .projection(ProgressionTarget::Attribute(AttributeId::Endurance))
                    .unwrap()
                    .details,
                Some(TraitDetails::Attribute {
                    starting_value: to + amount
                })
            );
            assert_eq!(
                character.adopt_attribute_transfer(proposed),
                Err(AttributeTransferError::Stale)
            );
        }
    }

    #[test]
    fn pinned_requirement_and_limit_order_preserves_owner() {
        let character = character(10, 100);
        assert_eq!(
            character.propose_attribute_transfer(
                AttributeId::Strength,
                AttributeId::Endurance,
                true
            ),
            Err(AttributeTransferError::WieldRequirement)
        );
        assert_eq!(
            character.propose_attribute_transfer(
                AttributeId::Strength,
                AttributeId::Endurance,
                false
            ),
            Err(AttributeTransferError::SourceAtMinimum)
        );
        assert_eq!(character.revision(), 7);
        let character = self::character(11, 100);
        assert_eq!(
            character.propose_attribute_transfer(
                AttributeId::Strength,
                AttributeId::Endurance,
                false
            ),
            Err(AttributeTransferError::TargetAtMaximum)
        );
    }
}
