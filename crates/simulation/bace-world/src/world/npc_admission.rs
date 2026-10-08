//! Fresh scripted sources settle physically but remain unavailable to gameplay
//! until their exact accepted source/journal is bound by the runtime owner.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldNpcAdmissionHold {
    actor: EntityId,
    generation: u64,
}
impl World {
    pub fn validate_npc_admissions(
        &self,
        actors: impl Iterator<Item = EntityId>,
    ) -> Result<(), WorldError> {
        let mut ids = std::collections::BTreeSet::new();
        for actor in actors {
            if actor.0 == 0
                || !ids.insert(actor)
                || self.npc_admissions.contains_key(&actor)
                || self
                    .combatants
                    .get(&actor)
                    .is_some_and(|c| c.profile().player)
            {
                return Err(WorldError::InvalidMotion);
            }
            if ids.len() > 4096 {
                return Err(WorldError::InvalidMotion);
            }
        }
        if self.npc_admissions.len() + ids.len() > 4096
            || self
                .next_npc_admission
                .checked_add(ids.len() as u64)
                .is_none()
        {
            return Err(WorldError::InvalidMotion);
        }
        Ok(())
    }
    pub fn begin_npc_admission(
        &mut self,
        actor: EntityId,
    ) -> Result<WorldNpcAdmissionHold, WorldError> {
        self.validate_npc_admissions(std::iter::once(actor))?;
        if !self.actors.contains_key(&actor) {
            return Err(WorldError::MissingActor);
        }
        self.next_npc_admission = self
            .next_npc_admission
            .checked_add(1)
            .ok_or(WorldError::InvalidMotion)?;
        let hold = WorldNpcAdmissionHold {
            actor,
            generation: self.next_npc_admission,
        };
        self.npc_admissions.insert(actor, hold);
        self.visibility.invalidate();
        Ok(hold)
    }
    pub fn finish_npc_admission(
        &mut self,
        actor: EntityId,
        hold: WorldNpcAdmissionHold,
    ) -> Result<(), WorldError> {
        if hold.actor != actor || self.npc_admissions.get(&actor) != Some(&hold) {
            return Err(WorldError::InvalidMotion);
        }
        self.npc_admissions.remove(&actor);
        self.visibility.invalidate();
        Ok(())
    }
    pub fn npc_admission_hold(&self, actor: EntityId) -> Option<WorldNpcAdmissionHold> {
        self.npc_admissions.get(&actor).copied()
    }
}
