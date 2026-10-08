//! Source queued quest awards join the shared reward transaction after immediate emote rows.
use super::Kernel;
use bace_gameplay_api::social::{EarnedExperience, RewardSharing, RewardXpKind, SocialError};
impl Kernel {
    pub(super) fn stage_npc_shared_rewards(&mut self) -> Result<(), SocialError> {
        if self.allegiances.pending.is_some() || !self.npcs.shared_experience {
            return Ok(());
        }
        let Some(queued) = self.npcs.ready_shared_experience() else {
            return Ok(());
        };
        let crate::NpcEffect::QueuedExperience {
            actor,
            amount,
            share,
            ..
        } = queued.effect
        else {
            return Err(SocialError::Invalid);
        };
        let mut ticket = match self.prepare_shared_experience(EarnedExperience {
            source: actor,
            amount,
            kind: RewardXpKind::Quest,
            sharing: RewardSharing {
                fellowship: share == crate::npc::NpcExperienceSharing::All,
                allegiance: share != crate::npc::NpcExperienceSharing::None,
            },
        }) {
            Ok(t) => t,
            Err(SocialError::Busy | SocialError::Capacity) => return Ok(()),
            Err(e) => return Err(e),
        };
        let change = ticket
            .player_changes
            .iter()
            .find(|(id, _)| *id == actor)
            .ok_or(SocialError::Invalid)?
            .1
            .clone();
        let prepared = self
            .npcs
            .attach_shared_experience(&queued, change)
            .map_err(|_| SocialError::Stale)?;
        ticket.npc = Some(prepared);
        self.allegiances.pending = Some(ticket);
        Ok(())
    }
}
