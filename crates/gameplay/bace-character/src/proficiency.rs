//! ACE Entity/Proficiency.OnSuccessUse, including queued GrantXP ordering.
use crate::{
    CharacterLevelTable, CharacterProgression, CharacterServiceError, CharacterServiceState,
    EarnedExperienceChange,
};
use bace_gameplay_api::{ProgressionProjection, ProgressionTarget, SkillAdvancement, TraitDetails};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProficiencyUse {
    pub skill: u32,
    pub difficulty: u32,
    pub unix_time: f64,
    pub olthoi: bool,
}
/// One immutable aggregate delta. GrantXP queues its award; the immediate skill
/// spend therefore uses pre-award available XP, even when that makes it fail.
#[derive(Clone, Debug, PartialEq)]
pub struct ProficiencyChange {
    pub usage: ProficiencyUse,
    pub before: ProgressionProjection,
    pub after: ProgressionProjection,
    pub earned: EarnedExperienceChange,
    pub spent: u32,
    pub grants_experience: bool,
    pub additional_change: bool,
}
impl CharacterProgression {
    pub fn propose_proficiency(
        &self,
        services: &CharacterServiceState,
        levels: &CharacterLevelTable,
        usage: ProficiencyUse,
        additional_change: bool,
    ) -> Result<Option<ProficiencyChange>, CharacterServiceError> {
        use CharacterServiceError as E;
        if !usage.unix_time.is_finite() {
            return Err(E::Invalid);
        }
        if usage.olthoi {
            return Ok(None);
        }
        let target = ProgressionTarget::Skill(usage.skill);
        let before = self.projection(target).ok_or(E::Invalid)?;
        if !matches!(
            before.advancement,
            SkillAdvancement::Trained | SkillAdvancement::Specialized
        ) {
            return Ok(None);
        }
        let Some(TraitDetails::Skill {
            initial_level,
            resistance_at_last_check,
            last_used_time,
        }) = before.details
        else {
            return Err(E::Invalid);
        };
        let elapsed = usage.unix_time - last_used_time;
        if elapsed >= 0.0 && usage.difficulty <= resistance_at_last_check && elapsed < 900.0 {
            return Ok(None);
        }
        let mut after = before;
        after.details = Some(TraitDetails::Skill {
            initial_level,
            resistance_at_last_check: if elapsed < 0.0 {
                resistance_at_last_check
            } else {
                usage.difficulty
            },
            last_used_time: usage.unix_time,
        });
        let maximum = levels.maximum_level();
        let grants_experience = elapsed >= 0.0 && services.level != maximum;
        let mut amount = 0;
        let mut points = 0;
        if grants_experience {
            let scale = if elapsed >= 900.0 {
                1.0_f32
            } else {
                (elapsed / 900.0) as f32
            };
            let raw = f64::from(usage.difficulty as f32 * scale).round_ties_even();
            if !(0.0..=u32::MAX as f64).contains(&raw) {
                return Err(E::Overflow);
            }
            points = raw as u32;
            amount = f64::from(points as f32 * 1.1_f32).round_ties_even() as u64;
            let remaining = levels
                .maximum_experience()
                .checked_sub(services.total_experience)
                .ok_or(E::Invalid)?;
            if amount > remaining {
                amount = remaining;
                points = f64::from(amount as f32 / 1.1_f32).round_ties_even() as u32;
            }
            let table = crate::progression::table_for(&self.tables, &self.traits[&target].progress)
                .ok_or(E::Invalid)?;
            points = points.min(
                table
                    .maximum_experience()
                    .saturating_sub(before.experience_spent),
            );
        }
        // HandleActionRaiseSkill executes before the queued UpdateXpAndLevel.
        let spent = if u64::from(points) <= self.available_experience {
            points
        } else {
            0
        };
        let mut candidate = self.pending_copy();
        candidate.available_experience -= u64::from(spent);
        candidate
            .traits
            .get_mut(&target)
            .ok_or(E::Invalid)?
            .progress
            .experience_spent += spent;
        candidate.traits.get_mut(&target).ok_or(E::Invalid)?.details = after.details;
        after = candidate.projection(target).ok_or(E::Invalid)?;
        let mut earned = if grants_experience {
            candidate.propose_earned_experience(services, levels, amount)?
        } else {
            EarnedExperienceChange {
                experience: candidate
                    .propose_experience_credit(0)
                    .map_err(|_| E::Overflow)?,
                services: crate::CharacterServiceChange {
                    before_revision: self.revision,
                    after_revision: self.revision,
                    before: services.clone(),
                    after: services.clone(),
                },
                before_skill_credits: self.available_skill_credits(),
                after_skill_credits: self.available_skill_credits(),
                earned_skill_credits: 0,
                credited: 0,
            }
        };
        let changed = additional_change || before != after || spent != 0 || earned.credited != 0;
        let revision = self
            .revision
            .checked_add(u64::from(changed))
            .ok_or(E::Overflow)?;
        earned.experience.before_available = self.available_experience;
        earned.experience.before_revision = self.revision;
        earned.experience.after_revision = revision;
        earned.services.before_revision = self.revision;
        earned.services.after_revision = revision;
        Ok(Some(ProficiencyChange {
            usage,
            before,
            after,
            earned,
            spent,
            grants_experience,
            additional_change,
        }))
    }
    /// Read-only view of a private pending candidate, for admission of derived
    /// combat/magic projections before any durable write is submitted.
    pub fn inspect_proficiency<R>(
        &self,
        services: &CharacterServiceState,
        levels: &CharacterLevelTable,
        change: &ProficiencyChange,
        inspect: impl FnOnce(&Self) -> R,
    ) -> Result<R, CharacterServiceError> {
        let mut candidate = self.pending_copy();
        let mut candidate_services = services.clone();
        candidate.adopt_proficiency(&mut candidate_services, levels, change)?;
        Ok(inspect(&candidate))
    }
    pub fn validate_proficiency(
        &self,
        services: &CharacterServiceState,
        levels: &CharacterLevelTable,
        change: &ProficiencyChange,
    ) -> Result<(), CharacterServiceError> {
        if self
            .propose_proficiency(services, levels, change.usage, change.additional_change)?
            .as_ref()
            != Some(change)
        {
            return Err(CharacterServiceError::Conflict);
        }
        Ok(())
    }
    pub fn adopt_proficiency(
        &mut self,
        services: &mut CharacterServiceState,
        levels: &CharacterLevelTable,
        change: &ProficiencyChange,
    ) -> Result<(), CharacterServiceError> {
        self.validate_proficiency(services, levels, change)?;
        let entry = self
            .traits
            .get_mut(&change.after.target)
            .ok_or(CharacterServiceError::Conflict)?;
        entry.progress.experience_spent = change.after.experience_spent;
        entry.details = change.after.details;
        self.available_experience = change.earned.experience.after_available;
        self.revision = change.earned.experience.after_revision;
        if let Some(credits) = change.earned.after_skill_credits {
            self.training
                .as_mut()
                .ok_or(CharacterServiceError::Conflict)?
                .credits = credits;
        }
        *services = change.earned.services.after.clone();
        Ok(())
    }
}
