//! Shutdown releases immutable registration state only after all owned work drains.
use super::*;
#[cfg(test)]
mod tests;
impl Npcs {
    /// Ordinary body retirement may discard only a fully acknowledged script.
    /// DeleteSelf archives have a separate continuation owner and never enter
    /// this path, even after their last row has completed.
    pub(crate) fn can_retire_idle_source(&self, actor: EntityId) -> bool {
        let references =
            |context: &NpcContext| context.source == actor || context.target == Some(actor);
        !self.pending_participant(actor)
            && !self.archives.contains_key(&actor)
            && self.sources.get(&actor).is_none_or(|source| {
                source.recovery_ready
                    && source.admission_hold.is_none()
                    && !source.manager.busy()
                    && !source.manager.has_detached()
            })
            && !self.proposals.iter().any(|p| references(&p.context))
            && !self.notifications.iter().any(|p| references(&p.context))
            && !self.deletions.values().any(|p| references(&p.npc.context))
            && !self
                .portal_services
                .values()
                .any(|p| references(&p.npc.context))
            && !self
                .skill_resets
                .values()
                .any(|(p, _)| references(&p.npc.context))
            && !self
                .training_credit_services
                .values()
                .any(|p| references(&p.npc.context))
            && !self
                .spellbook_services
                .values()
                .any(|p| references(&p.npc.context))
            && !self
                .inventory_services
                .values()
                .any(|p| references(&p.npc.context))
            && !self.cast_services.values().any(|p| references(&p.context))
            && !self
                .motion_services
                .values()
                .any(|(p, id, _, _)| *id == actor || references(&p.context))
            && !self
                .moves
                .values()
                .any(|p| p.actor == actor || references(&p.proposal.context))
            && !self.damage_events.iter().any(|event| match event {
                crate::CombatEvent::Damage {
                    attacker, target, ..
                } => *attacker == Some(actor) || *target == actor,
                crate::CombatEvent::Finished { actor: source, .. } => *source == actor,
            })
    }

    pub(crate) fn retire_idle_source(&mut self, actor: EntityId) -> Result<(), NpcFailure> {
        if !self.can_retire_idle_source(actor) {
            return Err(NpcFailure::DurabilityPending);
        }
        if self.sources.remove(&actor).is_some() {
            self.source_order.retain(|id| *id != actor);
            self.quests.remove(&actor);
        }
        Ok(())
    }

    pub(crate) fn mark_checkpoint_complete(&mut self, source: EntityId) {
        self.durable_dirty.remove(&source);
    }

    pub(crate) fn recovery_held(&self, actor: EntityId) -> bool {
        self.sources
            .get(&actor)
            .is_some_and(|s| !s.recovery_ready && s.journal_hold.is_none())
    }

    pub(crate) fn restore_idle_quests(
        &mut self,
        actor: EntityId,
        quests: Option<QuestRegistry>,
    ) -> Result<(), NpcFailure> {
        let source = self.sources.get(&actor).ok_or(NpcFailure::MissingActor)?;
        if source.recovery_ready
            || source.manager.busy()
            || source.manager.has_detached()
            || source.journal_hold.is_some()
        {
            return Err(NpcFailure::Conflict);
        }
        if let Some(quests) = quests {
            if !self.quests.contains_key(&actor) && self.quests.len() >= self.capacity {
                return Err(NpcFailure::Capacity);
            }
            self.quests.insert(actor, quests);
        }
        Ok(())
    }
    pub(crate) fn hold_recovery(&mut self, actor: EntityId) -> Result<(), NpcFailure> {
        let source = self
            .sources
            .get_mut(&actor)
            .ok_or(NpcFailure::MissingActor)?;
        if source.manager.busy() || source.journal_hold.is_some() {
            return Err(NpcFailure::Conflict);
        }
        source.recovery_ready = false;
        Ok(())
    }
    pub(crate) fn discard_idle_sources(&mut self) -> Result<(), NpcFailure> {
        if !self.durable_dirty.is_empty()
            || !self.source_inventory.is_empty()
            || self.sources.values().any(|source| {
                source.manager.busy()
                    || source.manager.has_detached()
                    || source.journal_hold.is_some()
                    || !source.recovery_ready
            })
            || !self.pending.is_empty()
            || !self.proposals.is_empty()
            || !self.notifications.is_empty()
            || !self.archives.is_empty()
            || !self.deletions.is_empty()
            || !self.deletion_retired.is_empty()
            || !self.deletion_committed.is_empty()
            || !self.deletion_registries.is_empty()
            || !self.damage_events.is_empty()
            || !self.portal_services.is_empty()
            || !self.skill_resets.is_empty()
            || !self.handins.is_empty()
            || !self.training_credit_services.is_empty()
            || !self.spellbook_services.is_empty()
            || !self.spellbook_registries.is_empty()
            || !self.moves.is_empty()
            || !self.inventory_services.is_empty()
            || !self.cast_services.is_empty()
            || !self.cast_completions.is_empty()
            || !self.motion_services.is_empty()
            || !self.motion_completions.is_empty()
            || !self.resumed.is_empty()
        {
            return Err(NpcFailure::DurabilityPending);
        }
        self.sources.clear();
        self.source_order.clear();
        self.quests.clear();
        self.scratch.clear();
        Ok(())
    }
}
