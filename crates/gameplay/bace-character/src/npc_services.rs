//! Character-owned NPC service state. Frozen adapters map titles to existing
//! character metadata and numeric/position values to accepted entity properties.
use bace_gameplay_api::NpcDestination;
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterServiceState {
    pub level: u32,
    pub total_experience: u64,
    pub titles: Vec<u32>,
    pub enlightenment: u32,
    pub sanctuary: Option<NpcDestination>,
    /// Nullable signed PropertyInt.TotalSkillCredits (23); absent stays absent.
    pub total_skill_credits: Option<i32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterServiceChange {
    pub before_revision: u64,
    pub after_revision: u64,
    pub before: CharacterServiceState,
    pub after: CharacterServiceState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterServiceError {
    Invalid,
    Capacity,
    Conflict,
    Overflow,
}
/// Player_Skills.AddSkillCredits changes nullable total and available counters
/// together. Negative authored debits are checked against the native unsigned
/// available-credit representation rather than wrapping it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrainingCreditChange {
    pub before_revision: u64,
    pub after_revision: u64,
    pub amount: i32,
    pub before_available: Option<u32>,
    pub after_available: Option<u32>,
    pub before_total: Option<i32>,
    pub after_total: Option<i32>,
}
impl crate::CharacterProgression {
    pub fn propose_training_credits(
        &self,
        state: &CharacterServiceState,
        amount: i32,
    ) -> Result<TrainingCreditChange, CharacterServiceError> {
        state.validate()?;
        let before_available = self.available_skill_credits();
        let after_available = before_available
            .map(|n| {
                let value = i64::from(n) + i64::from(amount);
                u32::try_from(value)
                    .ok()
                    .filter(|v| *v <= i32::MAX as u32)
                    .ok_or(CharacterServiceError::Overflow)
            })
            .transpose()?;
        let after_total = state
            .total_skill_credits
            .map(|v| v.checked_add(amount).ok_or(CharacterServiceError::Overflow))
            .transpose()?;
        let changed =
            before_available != after_available || state.total_skill_credits != after_total;
        Ok(TrainingCreditChange {
            before_revision: self.revision,
            after_revision: self
                .revision
                .checked_add(u64::from(changed))
                .ok_or(CharacterServiceError::Overflow)?,
            amount,
            before_available,
            after_available,
            before_total: state.total_skill_credits,
            after_total,
        })
    }
    pub fn adopt_training_credits(
        &mut self,
        state: &mut CharacterServiceState,
        change: &TrainingCreditChange,
    ) -> Result<(), CharacterServiceError> {
        if &self.propose_training_credits(state, change.amount)? != change {
            return Err(CharacterServiceError::Conflict);
        }
        if let Some(after) = change.after_available {
            self.training
                .as_mut()
                .ok_or(CharacterServiceError::Conflict)?
                .credits = after;
        }
        state.total_skill_credits = change.after_total;
        self.revision = change.after_revision;
        Ok(())
    }
}
impl CharacterServiceState {
    pub fn validate(&self) -> Result<(), CharacterServiceError> {
        if self.level == 0
            || self.level > 10000
            || self.total_experience > i64::MAX as u64
            || self.titles.len() > 4096
            || self.titles.windows(2).any(|v| v[0] >= v[1])
            || self.titles.contains(&0)
        {
            return Err(CharacterServiceError::Invalid);
        }
        if self.sanctuary.is_some_and(|p| {
            p.cell.is_none_or(|c| c.0 == 0)
                || p.relative
                || !p.position.is_finite()
                || p.rotation.iter().any(|v| !v.is_finite())
        }) {
            return Err(CharacterServiceError::Invalid);
        }
        Ok(())
    }
    pub fn propose_title(
        &self,
        revision: u64,
        title: u32,
    ) -> Result<CharacterServiceChange, CharacterServiceError> {
        self.validate()?;
        let mut after = self.clone();
        if title != 0
            && let Err(index) = after.titles.binary_search(&title)
        {
            if after.titles.len() == 4096 {
                return Err(CharacterServiceError::Capacity);
            }
            after.titles.insert(index, title);
        }
        self.change(revision, after)
    }
    pub fn propose_sanctuary(
        &self,
        revision: u64,
        destination: NpcDestination,
    ) -> Result<CharacterServiceChange, CharacterServiceError> {
        let mut after = self.clone();
        after.sanctuary = Some(destination);
        after.validate()?;
        self.change(revision, after)
    }
    fn change(
        &self,
        revision: u64,
        after: Self,
    ) -> Result<CharacterServiceChange, CharacterServiceError> {
        let after_revision = if *self == after {
            revision
        } else {
            revision
                .checked_add(1)
                .ok_or(CharacterServiceError::Overflow)?
        };
        Ok(CharacterServiceChange {
            before_revision: revision,
            after_revision,
            before: self.clone(),
            after,
        })
    }
}
/// Player_Xp.GrantLevelProportionalXp, with already accepted DAT next-level XP.
pub fn level_proportional_xp(
    next_level_xp: u64,
    percent: f64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, CharacterServiceError> {
    if !percent.is_finite() || percent < 0.0 || next_level_xp > i64::MAX as u64 {
        return Err(CharacterServiceError::Invalid);
    }
    let value = (next_level_xp as f64 * percent).round_ties_even();
    if value > i64::MAX as f64 {
        return Err(CharacterServiceError::Overflow);
    }
    let mut amount = value as i64;
    if maximum > 0 {
        amount = amount.min(maximum);
    }
    if minimum > 0 {
        amount = amount.max(minimum);
    }
    Ok(amount)
}
#[derive(Clone, Debug)]
pub struct CharacterLevelTable {
    xp: Vec<u64>,
    credits: Vec<u32>,
}
impl CharacterLevelTable {
    pub fn prepare(xp: Vec<u64>, credits: Vec<u32>) -> Result<Self, CharacterServiceError> {
        if xp.len() < 2
            || xp.len() > 10001
            || credits.len() != xp.len()
            || xp[0] != 0
            || xp.windows(2).any(|p| p[0] > p[1])
            || xp.last().is_some_and(|v| *v > i64::MAX as u64)
            || credits
                .iter()
                .try_fold(0u32, |a, v| a.checked_add(*v))
                .is_none_or(|v| v > i32::MAX as u32)
        {
            return Err(CharacterServiceError::Invalid);
        }
        Ok(Self { xp, credits })
    }
    pub fn maximum_experience(&self) -> u64 {
        self.xp[self.xp.len() - 1]
    }
    pub fn maximum_level(&self) -> u32 {
        (self.xp.len() - 1) as u32
    }
    pub fn next_skill_credit_level(&self, level: u32) -> u32 {
        self.credits
            .iter()
            .enumerate()
            .skip(level as usize + 1)
            .find_map(|(level, credits)| (*credits > 0).then_some(level as u32))
            .unwrap_or(0)
    }
    pub fn next_level_experience(&self, level: u32) -> u64 {
        let maximum = self.xp.len() - 1;
        let a = (level as usize).clamp(1, maximum.saturating_sub(1).max(1));
        let b = (level as usize + 1).clamp(1, maximum);
        self.xp[b] - self.xp[a]
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct EarnedExperienceChange {
    pub experience: crate::ExperienceCredit,
    pub services: CharacterServiceChange,
    pub before_skill_credits: Option<u32>,
    pub after_skill_credits: Option<u32>,
    pub earned_skill_credits: u32,
    pub credited: u64,
}
impl crate::CharacterProgression {
    /// UpdateXpAndLevel on one recipient after sharing/modifiers. The caller owns
    /// independent item, allegiance and vitae stages at source-defined times.
    pub fn propose_earned_experience(
        &self,
        state: &CharacterServiceState,
        table: &CharacterLevelTable,
        amount: u64,
    ) -> Result<EarnedExperienceChange, CharacterServiceError> {
        state.validate()?;
        let maximum = table.xp.len() - 1;
        if state.level as usize > maximum || state.total_experience > table.xp[maximum] {
            return Err(CharacterServiceError::Invalid);
        }
        let credited = if state.level as usize == maximum {
            0
        } else {
            amount.min(table.xp[maximum] - state.total_experience)
        };
        let mut after = state.clone();
        after.total_experience += credited;
        let mut credits = 0u32;
        while (after.level as usize) < maximum
            && after.total_experience >= table.xp[after.level as usize + 1]
        {
            after.level += 1;
            credits = credits
                .checked_add(table.credits[after.level as usize])
                .ok_or(CharacterServiceError::Overflow)?;
        }
        let signed_credits = i32::try_from(credits).map_err(|_| CharacterServiceError::Overflow)?;
        after.total_skill_credits = state
            .total_skill_credits
            .map(|before| {
                before
                    .checked_add(signed_credits)
                    .ok_or(CharacterServiceError::Overflow)
            })
            .transpose()?;
        let before_skill_credits = self.available_skill_credits();
        let after_skill_credits = match (before_skill_credits, credits) {
            (Some(before), delta) => Some(
                before
                    .checked_add(delta)
                    .filter(|v| *v <= i32::MAX as u32)
                    .ok_or(CharacterServiceError::Overflow)?,
            ),
            (None, 0) => None,
            _ => return Err(CharacterServiceError::Invalid),
        };
        let mut experience = self
            .propose_experience_credit(credited)
            .map_err(|_| CharacterServiceError::Overflow)?;
        let changed =
            state != &after || credited != 0 || before_skill_credits != after_skill_credits;
        experience.after_revision = self
            .revision
            .checked_add(u64::from(changed))
            .ok_or(CharacterServiceError::Overflow)?;
        let services = CharacterServiceChange {
            before_revision: self.revision,
            after_revision: experience.after_revision,
            before: state.clone(),
            after,
        };
        Ok(EarnedExperienceChange {
            experience,
            services,
            before_skill_credits,
            after_skill_credits,
            earned_skill_credits: credits,
            credited,
        })
    }
    pub fn adopt_earned_experience(
        &mut self,
        state: &mut CharacterServiceState,
        change: EarnedExperienceChange,
    ) -> Result<(), CharacterServiceError> {
        if self.revision != change.experience.before_revision
            || self.available_experience != change.experience.before_available
            || state != &change.services.before
            || self.available_skill_credits() != change.before_skill_credits
            || change.services.after_revision != change.experience.after_revision
        {
            return Err(CharacterServiceError::Conflict);
        }
        change.services.after.validate()?;
        let earned = i32::try_from(change.earned_skill_credits)
            .map_err(|_| CharacterServiceError::Overflow)?;
        if state.total_skill_credits.map(|v| v.checked_add(earned))
            != change.services.after.total_skill_credits.map(Some)
        {
            return Err(CharacterServiceError::Conflict);
        }
        let changed = change.experience.after_available != change.experience.before_available
            || change.services.before != change.services.after
            || change.before_skill_credits != change.after_skill_credits;
        if change.experience.after_revision
            != self
                .revision
                .checked_add(u64::from(changed))
                .ok_or(CharacterServiceError::Overflow)?
        {
            return Err(CharacterServiceError::Conflict);
        }
        if let Some(credits) = change.after_skill_credits {
            let training = self
                .training
                .as_mut()
                .ok_or(CharacterServiceError::Conflict)?;
            training.credits = credits;
        }
        self.available_experience = change.experience.after_available;
        self.revision = change.experience.after_revision;
        *state = change.services.after;
        Ok(())
    }
}
impl CharacterServiceState {
    pub fn scalar(
        &self,
        family: bace_gameplay_api::NpcPropertyFamily,
        stat: u32,
    ) -> Option<bace_gameplay_api::NpcValue> {
        use bace_gameplay_api::{NpcPropertyFamily as F, NpcValue as V};
        match (family, stat) {
            (F::Int, 23) => self.total_skill_credits.map(V::Int),
            (F::Int, 25) => Some(V::Int(self.level as i32)),
            (F::Int, 390) => i32::try_from(self.enlightenment).ok().map(V::Int),
            (F::Int64, 1) => Some(V::Int64(self.total_experience as i64)),
            _ => None,
        }
    }
    pub fn propose_scalar(
        &self,
        revision: u64,
        family: bace_gameplay_api::NpcPropertyFamily,
        stat: u32,
        mutation: bace_gameplay_api::NpcPropertyMutation,
    ) -> Result<CharacterServiceChange, CharacterServiceError> {
        use bace_gameplay_api::{NpcPropertyMutation as M, NpcValue as V};
        if family == bace_gameplay_api::NpcPropertyFamily::Int && stat == 23 {
            let mut after = self.clone();
            after.total_skill_credits = match mutation {
                M::Set {
                    family: f,
                    value: None,
                } if f == family => None,
                M::Set {
                    family: f,
                    value: Some(V::Int(value)),
                } if f == family => Some(value),
                M::AddInt(amount) => Some(
                    self.total_skill_credits
                        .unwrap_or(0)
                        .checked_add(amount)
                        .ok_or(CharacterServiceError::Overflow)?,
                ),
                _ => return Err(CharacterServiceError::Invalid),
            };
            return self.change(revision, after);
        }
        let before = self
            .scalar(family, stat)
            .ok_or(CharacterServiceError::Invalid)?;
        let value = match mutation {
            M::Set {
                family: f,
                value: Some(v),
            } if f == family => v,
            M::AddInt(amount) => match before {
                V::Int(before) => V::Int(
                    before
                        .checked_add(amount)
                        .ok_or(CharacterServiceError::Overflow)?,
                ),
                _ => return Err(CharacterServiceError::Invalid),
            },
            _ => return Err(CharacterServiceError::Invalid),
        };
        let mut after = self.clone();
        match (stat, value) {
            (25, V::Int(v)) => {
                after.level = u32::try_from(v).map_err(|_| CharacterServiceError::Invalid)?
            }
            (390, V::Int(v)) => {
                after.enlightenment =
                    u32::try_from(v).map_err(|_| CharacterServiceError::Invalid)?
            }
            (1, V::Int64(v)) => {
                after.total_experience =
                    u64::try_from(v).map_err(|_| CharacterServiceError::Invalid)?
            }
            _ => return Err(CharacterServiceError::Invalid),
        }
        after.validate()?;
        self.change(revision, after)
    }
}
