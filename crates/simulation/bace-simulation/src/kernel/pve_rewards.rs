//! Corpse and shared reward adoption share one externally confirmed transaction.
use super::Kernel;
use crate::{AllegianceTicket, PveError};
use bace_gameplay_api::social::{EarnedExperience, RewardSharing, RewardXpKind, SocialError};
use bace_types::EntityId;
impl Kernel {
    pub(super) fn stage_pve_shared_rewards(&mut self) -> Result<(), SocialError> {
        if self.allegiances.pending.is_some() {
            return Ok(());
        }
        let Some((operation, amounts, rare)) = self.population.waiting_shared_reward() else {
            return Ok(());
        };
        let mut rewards: Vec<_> = amounts
            .into_iter()
            .map(|(source, amount)| {
                u64::try_from(amount)
                    .map(|amount| EarnedExperience {
                        source,
                        amount,
                        kind: RewardXpKind::Kill,
                        sharing: RewardSharing {
                            fellowship: true,
                            allegiance: true,
                        },
                    })
                    .map_err(|_| SocialError::Invalid)
            })
            .collect::<Result<_, _>>()?;
        if rewards.is_empty() {
            if let Some(r) = &rare {
                rewards.push(EarnedExperience {
                    source: EntityId(r.character),
                    amount: 0,
                    kind: RewardXpKind::Kill,
                    sharing: RewardSharing {
                        fellowship: false,
                        allegiance: true,
                    },
                });
            } else {
                self.population
                    .clear_empty_shared_reward(operation)
                    .map_err(|_| SocialError::Stale)?;
                return Ok(());
            }
        }
        let ticket = match self.prepare_shared_experience_batch(&rewards, rare) {
            Ok(ticket) => ticket,
            Err(SocialError::Busy | SocialError::Capacity) => return Ok(()),
            Err(error) => return Err(error),
        };
        self.population
            .attach_shared_reward(operation, ticket)
            .map_err(|_| SocialError::Stale)?;
        self.allegiances.pve_operation = Some(operation);
        Ok(())
    }
    pub fn confirm_shared_death_committed(
        &mut self,
        operation: u64,
        corpse: EntityId,
        items: &[EntityId],
        credits: &[(EntityId, bace_character::ExperienceCredit)],
        ticket: &AllegianceTicket,
    ) -> Result<(), PveError> {
        if self.allegiances.pve_operation != Some(operation)
            || self.population.shared_death_ticket(operation) != Some(ticket)
        {
            return Err(PveError::InvalidReceipt);
        }
        self.validate_allegiance_commit(ticket)
            .map_err(|_| PveError::InvalidReceipt)?;
        self.population.committed(
            operation,
            corpse,
            items,
            credits,
            &mut self.world,
            self.tick,
        )?;
        self.adopt_allegiance_commit(ticket)
            .expect("prevalidated same-owner shared reward commit");
        self.allegiances.pve_operation = None;
        Ok(())
    }
}
