//! Revision-fenced spendable-XP adoption after a confirmed durable operation.
//! Full ACE EarnXP level/fellowship/allegiance/rate processing remains separate.
use crate::CharacterProgression;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExperienceCredit {
    pub before_revision: u64,
    pub after_revision: u64,
    pub before_available: u64,
    pub after_available: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardCreditError {
    Overflow,
    Conflict,
}
impl CharacterProgression {
    pub fn propose_experience_credit(
        &self,
        amount: u64,
    ) -> Result<ExperienceCredit, RewardCreditError> {
        let after_available = self
            .available_experience
            .checked_add(amount)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or(RewardCreditError::Overflow)?;
        let after_revision = if amount == 0 {
            self.revision
        } else {
            self.revision
                .checked_add(1)
                .ok_or(RewardCreditError::Overflow)?
        };
        Ok(ExperienceCredit {
            before_revision: self.revision,
            after_revision,
            before_available: self.available_experience,
            after_available,
        })
    }
    pub fn propose_experience_spend(
        &self,
        amount: u64,
    ) -> Result<ExperienceCredit, RewardCreditError> {
        let after_available = self
            .available_experience
            .checked_sub(amount)
            .ok_or(RewardCreditError::Conflict)?;
        let after_revision = if amount == 0 {
            self.revision
        } else {
            self.revision
                .checked_add(1)
                .ok_or(RewardCreditError::Overflow)?
        };
        Ok(ExperienceCredit {
            before_revision: self.revision,
            after_revision,
            before_available: self.available_experience,
            after_available,
        })
    }
    /// The save adapter must persist these exact after-values before adoption.
    /// Reserving the character between proposal and confirmation is mandatory.
    pub fn adopt_experience_credit(
        &mut self,
        credit: ExperienceCredit,
    ) -> Result<(), RewardCreditError> {
        self.adopt_combined_reward(credit, false)
    }
}

impl CharacterProgression {
    /// One aggregate revision covers XP plus a committed rare-state mutation.
    pub fn adopt_combined_reward(
        &mut self,
        credit: ExperienceCredit,
        additional_persistent_change: bool,
    ) -> Result<(), RewardCreditError> {
        if self.revision != credit.before_revision
            || self.available_experience != credit.before_available
            || credit.after_available > i64::MAX as u64
        {
            return Err(RewardCreditError::Conflict);
        }
        let changed =
            credit.before_available != credit.after_available || additional_persistent_change;
        let revision = if changed {
            self.revision
                .checked_add(1)
                .ok_or(RewardCreditError::Overflow)?
        } else {
            self.revision
        };
        if revision != credit.after_revision {
            return Err(RewardCreditError::Conflict);
        }
        self.available_experience = credit.after_available;
        self.revision = revision;
        Ok(())
    }
}
