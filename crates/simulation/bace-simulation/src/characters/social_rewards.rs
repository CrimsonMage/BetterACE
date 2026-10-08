//! Distinct reservation domain prevents numerically equal PvE/social operation IDs aliasing.
use super::*;
use bace_character::{CharacterLevelTable, EarnedExperienceChange};
impl Characters {
    pub(crate) fn social_reward_operation(&self, actor: EntityId) -> Option<u64> {
        self.entries.get(&actor)?.social_reward
    }

    pub(crate) fn reserve_social_rewards(
        &mut self,
        operation: u64,
        changes: &[(EntityId, EarnedExperienceChange)],
        table: &CharacterLevelTable,
    ) -> Result<(), ()> {
        let mut ids = std::collections::BTreeSet::new();
        for (actor, change) in changes {
            if !ids.insert(*actor)
                || self.reserved(*actor)
                || self.earned_experience(*actor, table, change.credited)? != *change
            {
                return Err(());
            }
        }
        for (actor, _) in changes {
            self.entries.get_mut(actor).ok_or(())?.social_reward = Some(operation);
        }
        Ok(())
    }
    pub(crate) fn validate_social_rewards(
        &self,
        operation: u64,
        changes: &[(EntityId, EarnedExperienceChange)],
        table: &CharacterLevelTable,
    ) -> Result<(), ()> {
        for (actor, change) in changes {
            let entry = self.entries.get(actor).ok_or(())?;
            if entry.social_reward != Some(operation)
                || self.earned_experience(*actor, table, change.credited)? != *change
            {
                return Err(());
            }
        }
        Ok(())
    }
    pub(crate) fn adopt_social_rewards(
        &mut self,
        operation: u64,
        changes: &[(EntityId, EarnedExperienceChange)],
        table: &CharacterLevelTable,
    ) -> Result<(), ()> {
        self.validate_social_rewards(operation, changes, table)?;
        for (actor, change) in changes {
            self.adopt_earned_experience(*actor, change.clone())
                .expect("validated same-owner earned XP");
            self.entries
                .get_mut(actor)
                .expect("validated actor")
                .social_reward = None;
        }
        Ok(())
    }
    pub(crate) fn reject_social_rewards(
        &mut self,
        operation: u64,
        changes: &[(EntityId, EarnedExperienceChange)],
    ) -> Result<(), ()> {
        if changes.iter().any(|(actor, _)| {
            self.entries
                .get(actor)
                .is_none_or(|e| e.social_reward != Some(operation))
        }) {
            return Err(());
        }
        for (actor, _) in changes {
            self.entries
                .get_mut(actor)
                .expect("validated actor")
                .social_reward = None;
        }
        Ok(())
    }
}
impl Characters {
    pub(crate) fn validate_social_rare(
        &self,
        rare: &bace_gameplay_api::RareDecision,
    ) -> Result<(), ()> {
        let entry = self.entries.get(&EntityId(rare.character)).ok_or(())?;
        if entry.rare != Some(rare.previous) || entry.rare_pending.is_some() {
            return Err(());
        }
        Ok(())
    }
    pub(crate) fn adopt_social_rare(
        &mut self,
        rare: &bace_gameplay_api::RareDecision,
        _experience: &bace_character::ExperienceCredit,
    ) -> Result<(), ()> {
        self.validate_social_rare(rare)?;
        let entry = self.entries.get_mut(&EntityId(rare.character)).ok_or(())?;
        entry.rare = Some(rare.next);
        Ok(())
    }
}

impl Characters {
    pub(crate) fn adopt_social_revision(
        &mut self,
        actor: EntityId,
        revision: u64,
    ) -> Result<(), ()> {
        let e = self.entries.get_mut(&actor).ok_or(())?;
        if e.progression.revision() == revision {
            return Ok(());
        }
        if e.progression.revision().checked_add(1) != Some(revision) {
            return Err(());
        }
        e.progression.touch_revision().map_err(|_| ())?;
        Ok(())
    }
}
