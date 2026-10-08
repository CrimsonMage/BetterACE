//! Cold NPC caster metadata retires only after owned continuations/output drain.
//! World-owned emitted projectiles keep their own frozen identity/lifetime.
use super::*;
impl Magic {
    pub(crate) fn can_retire_npc(&self, actor: EntityId) -> bool {
        !self.casters.get(&actor).is_some_and(|p|p.player)
            && !self.damage_profiles.get(&actor).is_some_and(|p|p.player)
            && !self.attempts.values().chain(self.instant_continuations.values()).any(|a|a.origin.actor()==actor||a.target==Some(actor))
            && !self.component_operations.values().any(|o|o.actor==actor)
            && !self.proc_involves(actor)
            && !self.periodic_involves(actor)
            && !self.claimed_emote_outcomes.iter().any(|(id,_)|*id==actor)
            && !self.outcomes.iter().any(|o|o.context.actor==actor)
            && !self.server_outcomes.iter().any(|o|o.origin.actor()==actor)
            && !self.events.iter().any(|event|registry::event_actor(event)==actor)
            && !self.combat.iter().any(|event|matches!(event,CombatEvent::Damage{attacker,target,..}if *attacker==Some(actor)||*target==actor))
            && !self.flying.values().any(|f|f.source==actor&&(f.pending_damage.is_some()||f.pending_impact.is_some()))
    }
    pub(crate) fn retire_npc(&mut self, actor: EntityId) -> Result<(), CastRejection> {
        if !self.can_retire_npc(actor) || self.registries.contains_key(&actor) {
            return Err(CastRejection::Busy);
        }
        self.clear_npc_metadata(actor);
        Ok(())
    }
    pub(crate) fn rollback_fresh_npc(&mut self, actor: EntityId) -> Result<(), CastRejection> {
        if !self.can_retire_npc(actor) || self.registry_reserved(actor) {
            return Err(CastRejection::Busy);
        }
        self.clear_npc_metadata(actor);
        Ok(())
    }
    fn clear_npc_metadata(&mut self, actor: EntityId) {
        self.casters.remove(&actor);
        self.object_casters.remove(&actor);
        self.server_mana.remove(&actor);
        self.recovery.remove(&actor);
        self.monster_ai.remove(&actor);
        self.defense_profiles.remove(&actor);
        self.damage_profiles.remove(&actor);
        self.periodic_profiles.remove(&actor);
        self.vitae_sequences.remove(&actor);
        self.vitae_removals.remove(&actor);
        self.server_sequences.retain(|(id, _), _| *id != actor);
        self.actor_components.retain(|(id, _), _| *id != actor);
        self.validated_actor_programs.retain(|(id, _)| *id != actor);
    }
}
impl Magic {
    pub(crate) fn can_retire_npc_registry(
        &self,
        actor: EntityId,
        now: f64,
    ) -> Result<(), CastRejection> {
        self.check_time(now)?;
        let clock = self
            .registry_clocks
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?;
        if !clock.reserved || clock.error.is_some() || !self.can_retire_npc(actor) {
            return Err(CastRejection::Busy);
        }
        Ok(())
    }
    /// Caller owns the named NPC lifecycle/retirement registry hold. Ordinary
    /// item tombstones still cannot retire a caster through the generic API.
    pub(crate) fn retire_npc_registry(
        &mut self,
        actor: EntityId,
        now: f64,
    ) -> Result<(), CastRejection> {
        self.can_retire_npc_registry(actor, now)?;
        self.registries
            .remove(&actor)
            .ok_or(CastRejection::MissingActor)?;
        self.registry_clocks.remove(&actor);
        Ok(())
    }
}
