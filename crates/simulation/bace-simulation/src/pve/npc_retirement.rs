//! Scripted retirement preserves population ownership and rejects a pending
//! death, replacement incarnation, held vital or unfinished world motion.
use super::*;
impl Population {
    pub(crate) fn can_remove_scripted(
        &self,
        actor: EntityId,
        origin: Option<GeneratedNpcOrigin>,
        world: &World,
    ) -> Result<(), PveError> {
        if self.npcs.get(&actor).is_none_or(|n| n.origin != origin) || world.body(actor).is_err() {
            return Err(PveError::MissingActor);
        }
        if self.pending.values().any(|p| p.proposal.victim == actor)
            || world.has_reserved_vitals(actor)
            || !world.can_retire_actor_motion(actor)
        {
            return Err(PveError::InvalidReceipt);
        }
        Ok(())
    }
    pub(crate) fn can_remove_scripted_held(
        &self,
        hold: bace_world::WorldRetirementHold,
        origin: Option<GeneratedNpcOrigin>,
        world: &World,
    ) -> Result<(), PveError> {
        if self
            .npcs
            .get(&hold.actor)
            .is_none_or(|n| n.origin != origin)
            || world.retirement_hold(hold.actor) != Some(hold)
            || world.has_reserved_vitals_except(hold.actor, hold.vital_token())
            || self
                .pending
                .values()
                .any(|p| p.proposal.victim == hold.actor)
        {
            return Err(PveError::InvalidReceipt);
        }
        Ok(())
    }
    pub(crate) fn remove_scripted_held(
        &mut self,
        hold: bace_world::WorldRetirementHold,
        origin: Option<GeneratedNpcOrigin>,
        world: &mut World,
    ) -> Result<(), PveError> {
        self.can_remove_scripted_held(hold, origin, world)?;
        world
            .remove_retired(hold)
            .map_err(|_| PveError::InvalidReceipt)?;
        self.npcs.remove(&hold.actor);
        self.native.retired(hold.actor);
        Ok(())
    }
}
