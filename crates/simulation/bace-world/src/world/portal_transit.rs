//! Server-only portal-space ownership, fenced by the portal service operation.
use super::*;
impl World {
    pub fn begin_portal_transit(
        &mut self,
        actors: &[(EntityId, u16)],
        token: u64,
    ) -> Result<(), WorldError> {
        if token == 0 || actors.is_empty() || actors.len() > 9 {
            return Err(WorldError::InvalidTeleportBatch);
        }
        let mut ids = std::collections::BTreeSet::new();
        for (id, epoch) in actors {
            if !ids.insert(*id) || self.anchors.contains(id) {
                return Err(WorldError::InvalidTeleportBatch);
            }
            let actor = self.actors.get(id).ok_or(WorldError::MissingActor)?;
            if let Some(old) = self.portal_transit.get(id) {
                if *old != token {
                    return Err(WorldError::InvalidTeleportBatch);
                }
            } else if actor.body.accepted().epoch() != *epoch {
                return Err(PhysicsError::StaleEpoch.into());
            }
        }
        let added = actors
            .iter()
            .filter(|(id, _)| !self.portal_transit.contains_key(id))
            .count();
        if self.portal_transit.len() + added > 4096 {
            return Err(WorldError::InvalidTeleportBatch);
        }
        for (id, _) in actors {
            self.portal_transit.insert(*id, token);
            self.actors
                .get_mut(id)
                .expect("preflight actor")
                .body
                .stop_motion();
        }
        Ok(())
    }
    pub fn finish_portal_transit(&mut self, actor: EntityId, token: u64) -> Result<(), WorldError> {
        if self.portal_transit.get(&actor) != Some(&token) {
            return Err(WorldError::InvalidTeleportBatch);
        }
        self.portal_transit.remove(&actor);
        Ok(())
    }
    pub fn is_in_portal_transit(&self, actor: EntityId) -> bool {
        self.portal_transit.contains_key(&actor)
            || self.entry_pending.contains(&actor)
            || self.npc_admissions.contains_key(&actor)
    }
}
