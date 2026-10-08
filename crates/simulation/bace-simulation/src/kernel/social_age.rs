//! Committed character playtime with deferred revision updates during valuable holds.
use super::*;
impl Kernel {
    pub fn social_player_age(&self, actor: EntityId) -> Option<u64> {
        self.social
            .chat_eligibility
            .get(&actor)
            .map(|(v, _)| v.player_age_seconds)
    }
    pub(in crate::kernel) fn sync_social_age(&mut self) -> Result<(), E> {
        for (actor, (eligibility, at)) in &mut self.social.chat_eligibility {
            let elapsed = self.tick.saturating_sub(*at) / 30;
            if elapsed == 0 {
                continue;
            }
            let next = eligibility
                .player_age_seconds
                .checked_add(elapsed)
                .filter(|v| *v <= i32::MAX as u64)
                .ok_or(E::Overflow)?;
            if !self
                .characters
                .touch_auxiliary(*actor)
                .map_err(|_| E::Overflow)?
            {
                continue;
            }
            eligibility.player_age_seconds = next;
            *at = at.checked_add(elapsed * 30).ok_or(E::Overflow)?;
        }
        Ok(())
    }
    pub fn configure_chat_policy(&mut self, policy: bace_social::ChatPolicy) -> Result<(), E> {
        if self.social.has_state() {
            return Err(E::Busy);
        }
        self.social.chat_policy = policy;
        Ok(())
    }
    pub fn register_chat_eligibility(
        &mut self,
        actor: EntityId,
        eligibility: bace_social::ChatEligibility,
    ) -> Result<(), E> {
        if !self
            .social
            .directory
            .presence(actor)
            .is_some_and(|p| p.online)
        {
            return Err(E::Missing);
        }
        if eligibility.account_created_unix.is_some_and(|v| v < 0)
            || eligibility.player_age_seconds > i32::MAX as u64
        {
            return Err(E::Invalid);
        }
        if self.social.chat_eligibility.contains_key(&actor) {
            return Err(E::Duplicate);
        }
        self.social
            .chat_eligibility
            .insert(actor, (eligibility, self.tick));
        Ok(())
    }
}
