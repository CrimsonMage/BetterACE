use super::*;
type WaitingSharedReward = (
    u64,
    Vec<(EntityId, i64)>,
    Option<bace_gameplay_api::RareDecision>,
);

impl Population {
    pub(crate) fn clear_empty_shared_reward(&mut self, operation: u64) -> Result<(), PveError> {
        let p = self
            .pending
            .get_mut(&operation)
            .filter(|p| {
                p.shared_waiting
                    && p.proposal.experience.is_empty()
                    && p.proposal.native.as_ref().is_none_or(|n| n.rare.is_none())
            })
            .ok_or(PveError::InvalidReceipt)?;
        p.shared_waiting = false;
        Ok(())
    }
    pub(crate) fn waiting_shared_reward(&self) -> Option<WaitingSharedReward> {
        self.pending
            .iter()
            .find(|(_, p)| p.shared_waiting)
            .map(|(id, p)| {
                (
                    *id,
                    p.proposal.experience.clone(),
                    p.proposal.native.as_ref().and_then(|n| n.rare.clone()),
                )
            })
    }
    pub(crate) fn attach_shared_reward(
        &mut self,
        operation: u64,
        ticket: crate::AllegianceTicket,
    ) -> Result<(), PveError> {
        let p = self
            .pending
            .get_mut(&operation)
            .filter(|p| p.shared_waiting && !p.submitted)
            .ok_or(PveError::InvalidReceipt)?;
        p.proposal.experience_state = ticket
            .player_changes
            .iter()
            .map(|(actor, c)| {
                let mut credit = c.experience;
                credit.after_revision = ticket
                    .player_revision(*actor)
                    .expect("prepared joined revision");
                (*actor, credit)
            })
            .collect();
        p.proposal.social = Some(ticket);
        p.shared_waiting = false;
        Ok(())
    }
    pub(crate) fn shared_death_ticket(&self, operation: u64) -> Option<&crate::AllegianceTicket> {
        self.pending.get(&operation)?.proposal.social.as_ref()
    }
}
