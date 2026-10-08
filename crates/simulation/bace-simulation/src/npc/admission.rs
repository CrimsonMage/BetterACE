//! Immutable script input is validated before actor publication. Runtime must
//! bind the exact accepted definition and restore its journal before execution.
use super::*;
#[cfg(test)]
mod tests;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcScriptIdentity {
    pub template: u32,
    pub program_hash: [u8; 32],
    pub content_generation: [u8; 32],
}
#[derive(Clone)]
pub struct PreparedNpcScriptSource {
    pub identity: NpcScriptIdentity,
    pub program: Arc<NativeProgram>,
    pub properties: bace_entity::EntityProperties,
    pub use_radius: f32,
}
impl PreparedNpcScriptSource {
    pub fn valid_bounds(&self) -> bool {
        self.identity.template != 0
            && self.identity.program_hash != [0; 32]
            && self.identity.content_generation != [0; 32]
            && self.use_radius.is_finite()
            && self
                .properties
                .retained_bytes()
                .is_some_and(|n| n <= 2 * 1024 * 1024)
    }
}
impl Npcs {
    pub(crate) fn admitted_same(&self, actor: EntityId, identity: NpcScriptIdentity) -> bool {
        self.sources
            .get(&actor)
            .is_some_and(|s| s.admission == Some(identity) && !s.recovery_ready)
    }
    pub(crate) fn admission_hold(
        &self,
        actor: EntityId,
    ) -> Option<bace_world::WorldNpcAdmissionHold> {
        self.sources.get(&actor).and_then(|s| s.admission_hold)
    }
    pub(crate) fn set_admission_hold(
        &mut self,
        actor: EntityId,
        hold: Option<bace_world::WorldNpcAdmissionHold>,
    ) {
        self.sources
            .get_mut(&actor)
            .expect("admitted source")
            .admission_hold = hold;
    }

    pub(crate) fn preflight_scripts<'a>(
        &self,
        scripts: impl Iterator<Item = (EntityId, &'a PreparedNpcScriptSource)>,
    ) -> Result<(), NpcFailure> {
        let mut ids = std::collections::BTreeSet::new();
        for (actor, script) in scripts {
            if actor.0 == 0
                || !script.valid_bounds()
                || !ids.insert(actor)
                || self.sources.contains_key(&actor)
            {
                return Err(NpcFailure::Conflict);
            }
        }
        if self.sources.len() + ids.len() > self.capacity
            || self.quests.len()
                + ids
                    .iter()
                    .filter(|id| !self.quests.contains_key(id))
                    .count()
                > 4096
        {
            return Err(NpcFailure::Capacity);
        }
        Ok(())
    }
    pub(crate) fn mark_admitted(
        &mut self,
        actor: EntityId,
        identity: NpcScriptIdentity,
    ) -> Result<(), NpcFailure> {
        let source = self
            .sources
            .get_mut(&actor)
            .ok_or(NpcFailure::MissingActor)?;
        source.admission = Some(identity);
        source.admission_bound = false;
        source.recovery_ready = false;
        Ok(())
    }
    pub(crate) fn bind_admitted(
        &mut self,
        actor: EntityId,
        identity: NpcScriptIdentity,
    ) -> Result<(), NpcFailure> {
        if self
            .sources
            .get(&actor)
            .is_none_or(|s| s.admission != Some(identity) || s.recovery_ready)
        {
            return Err(NpcFailure::Conflict);
        }
        self.sources
            .get_mut(&actor)
            .expect("bound source")
            .admission_bound = true;
        Ok(())
    }
    pub(crate) fn admission_pending(&self, actor: EntityId) -> bool {
        self.sources.get(&actor).is_some_and(|s| !s.recovery_ready)
    }
    pub(crate) fn rollback_admitted(&mut self, actor: EntityId) -> Result<(), NpcFailure> {
        let Some(source) = self.sources.get(&actor) else {
            return Ok(());
        };
        if source.admission.is_none() {
            return Ok(());
        }
        if source.recovery_ready
            || source.manager.busy()
            || source.random.is_some()
            || source.active_operation != 0
            || !source.invocations.is_empty()
            || source.journal_hold.is_some()
            || self
                .pending
                .values()
                .any(|p| p.proposal.context.source == actor)
            || self.proposals.iter().any(|p| p.context.source == actor)
        {
            return Err(NpcFailure::Conflict);
        }
        self.sources.remove(&actor);
        self.source_order.retain(|id| *id != actor);
        if self
            .quests
            .get(&actor)
            .is_some_and(|quests| quests.revision() == 0 && quests.iter().next().is_none())
        {
            self.quests.remove(&actor);
        }
        Ok(())
    }
}
