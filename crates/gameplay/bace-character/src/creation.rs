use bace_gameplay_api::{CreationAllocation, CreationAttributes, SkillAdvancement};

/// Resolved DAT skill costs after the selected heritage's overrides. These are
/// immutable trusted inputs, never prices submitted by the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreationSkillCosts {
    pub trained: u32,
    pub specialized: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreationRules {
    attribute_credits: u32,
    skill_credits: u32,
    skills: [Option<CreationSkillCosts>; 55],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationRulesError {
    CreditOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationRejection {
    AttributeOutOfRange,
    TooManyAttributeCredits,
    UnknownSkill(u32),
    InsufficientTrainingCredits(u32),
    InsufficientSpecializationCredits(u32),
}

/// Initial skill values produced by normal fresh-character allocation, before
/// template-derived attributes, enchantments or augmentation effects are added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatedSkill {
    pub advancement: SkillAdvancement,
    pub experience_spent: u32,
    pub ranks: u16,
    pub initial_level: u32,
}

/// Validated allocation only. It neither reserves a name nor creates/persists
/// any actor, equipment or account ownership record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedAllocation {
    pub attributes: CreationAttributes,
    pub skills: [Option<CreatedSkill>; 55],
    pub available_skill_credits: u32,
    pub total_skill_credits: u32,
}

impl CreationRules {
    pub fn new(
        attribute_credits: u32,
        skill_credits: u32,
        skills: [Option<CreationSkillCosts>; 55],
    ) -> Result<Self, CreationRulesError> {
        if skill_credits > i32::MAX as u32
            || skills
                .iter()
                .flatten()
                .any(|cost| cost.trained > i32::MAX as u32 || cost.specialized > i32::MAX as u32)
        {
            return Err(CreationRulesError::CreditOutOfRange);
        }
        Ok(Self {
            attribute_credits,
            skill_credits,
            skills,
        })
    }

    /// Validate the complete allocation before returning any proposed state.
    /// Like pinned ACE, unused credits are permitted and inactive skills do not
    /// require a DAT entry. No retail locked-skill policy is silently added.
    pub fn validate(
        &self,
        request: &CreationAllocation,
    ) -> Result<ValidatedAllocation, CreationRejection> {
        validate_attributes(request.attributes, self.attribute_credits)?;
        let mut credits = self.skill_credits;
        let mut skills = [None; 55];
        for (index, advancement) in request.skills.iter().copied().enumerate() {
            if advancement == SkillAdvancement::Inactive {
                continue;
            }
            let id = index as u32;
            let costs = self.skills[index].ok_or(CreationRejection::UnknownSkill(id))?;
            if matches!(
                advancement,
                SkillAdvancement::Trained | SkillAdvancement::Specialized
            ) {
                credits = credits
                    .checked_sub(costs.trained)
                    .ok_or(CreationRejection::InsufficientTrainingCredits(id))?;
            }
            if advancement == SkillAdvancement::Specialized {
                credits = credits
                    .checked_sub(costs.specialized)
                    .ok_or(CreationRejection::InsufficientSpecializationCredits(id))?;
            }
            skills[index] = Some(match advancement {
                SkillAdvancement::Trained => CreatedSkill {
                    advancement,
                    experience_spent: 526,
                    ranks: 5,
                    initial_level: 0,
                },
                SkillAdvancement::Specialized => CreatedSkill {
                    advancement,
                    experience_spent: 0,
                    ranks: 0,
                    initial_level: 10,
                },
                SkillAdvancement::Untrained | SkillAdvancement::Inactive => CreatedSkill {
                    advancement,
                    experience_spent: 0,
                    ranks: 0,
                    initial_level: 0,
                },
            });
        }
        Ok(ValidatedAllocation {
            attributes: request.attributes,
            skills,
            available_skill_credits: credits,
            total_skill_credits: self.skill_credits,
        })
    }
}

/// PlayerFactory.ValidateAttributeCredits: six values in 10..=100 and a total
/// no larger than the selected heritage's explicitly supplied credit budget.
pub fn validate_attributes(
    attributes: CreationAttributes,
    maximum: u32,
) -> Result<(), CreationRejection> {
    let values = attributes.wire_order();
    if values.iter().any(|value| !(10..=100).contains(value)) {
        return Err(CreationRejection::AttributeOutOfRange);
    }
    // Each term was checked above, so the sum cannot exceed 600.
    if values.iter().sum::<u32>() > maximum {
        return Err(CreationRejection::TooManyAttributeCredits);
    }
    Ok(())
}
