//! Pinned Player_Luminance.cs, ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! AGPL-3.0-only. Negative spending/grants and arithmetic overflow are rejected.
use crate::CharacterProgression;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuminanceState {
    pub available: i64,
    pub maximum: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuminanceCredit {
    pub before_revision: u64,
    pub after_revision: u64,
    pub before: LuminanceState,
    pub after: LuminanceState,
    pub requested: i64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LuminanceModifiers {
    pub luminance: f64,
    pub quest: f64,
    pub enchantment: f64,
    pub is_quest: bool,
    pub olthoi: bool,
}
impl Default for LuminanceModifiers {
    fn default() -> Self {
        Self {
            luminance: 1.0,
            quest: 1.0,
            enchantment: 1.0,
            is_quest: true,
            olthoi: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LuminanceError {
    MissingState,
    InvalidAmount,
    InvalidModifier,
    Overflow,
    Insufficient,
    Conflict,
}
impl CharacterProgression {
    pub fn with_luminance(mut self, state: LuminanceState) -> Result<Self, (LuminanceError, Self)> {
        if state.available < 0 || state.maximum < 0 || state.available > state.maximum {
            return Err((LuminanceError::InvalidAmount, self));
        }
        self.luminance = Some(state);
        Ok(self)
    }
    pub fn luminance(&self) -> Option<LuminanceState> {
        self.luminance
    }
    pub fn propose_luminance(
        &self,
        amount: i64,
        spend: bool,
        modifiers: LuminanceModifiers,
    ) -> Result<LuminanceCredit, LuminanceError> {
        let before = self.luminance.ok_or(LuminanceError::MissingState)?;
        if amount < 0 {
            return Err(LuminanceError::InvalidAmount);
        }
        let requested = if spend || modifiers.olthoi {
            amount
        } else {
            if [modifiers.luminance, modifiers.quest, modifiers.enchantment]
                .iter()
                .any(|m| !m.is_finite() || *m < 0.0)
            {
                return Err(LuminanceError::InvalidModifier);
            }
            let modifier = if modifiers.is_quest {
                modifiers.luminance * modifiers.quest
            } else {
                modifiers.luminance
            };
            let result = (amount as f64 * modifiers.enchantment * modifier).round_ties_even();
            if !result.is_finite() || !(0.0..9_223_372_036_854_775_808.0).contains(&result) {
                return Err(LuminanceError::Overflow);
            }
            result as i64
        };
        let available = if spend {
            before
                .available
                .checked_sub(requested)
                .filter(|v| *v >= 0)
                .ok_or(LuminanceError::Insufficient)?
        } else if modifiers.olthoi {
            before.available
        } else {
            before.available + requested.min(before.maximum - before.available)
        };
        let after = LuminanceState {
            available,
            ..before
        };
        let after_revision = if before == after {
            self.revision
        } else {
            self.revision
                .checked_add(1)
                .ok_or(LuminanceError::Overflow)?
        };
        Ok(LuminanceCredit {
            before_revision: self.revision,
            after_revision,
            before,
            after,
            requested,
        })
    }
    /// Requires the owning simulation reservation and exact confirmed save values.
    pub fn adopt_luminance(&mut self, credit: LuminanceCredit) -> Result<(), LuminanceError> {
        if self.revision != credit.before_revision
            || self.luminance != Some(credit.before)
            || credit.after.maximum != credit.before.maximum
            || credit.after.available < 0
            || credit.after.available > credit.after.maximum
        {
            return Err(LuminanceError::Conflict);
        }
        let revision = if credit.before == credit.after {
            self.revision
        } else {
            self.revision
                .checked_add(1)
                .ok_or(LuminanceError::Overflow)?
        };
        if revision != credit.after_revision {
            return Err(LuminanceError::Conflict);
        }
        self.luminance = Some(credit.after);
        self.revision = revision;
        Ok(())
    }
}
