//! Source registration is part of actual World admission, never delayed to an
//! observer callback that might arrive after AI/death has already referenced it.
#[cfg(test)]
mod tests;
use super::Kernel;
use bace_types::EntityId;
impl Kernel {
    pub(in crate::kernel) fn preflight_npc_scripts<'a>(
        &self,
        scripts: impl Iterator<Item = (EntityId, &'a crate::npc::PreparedNpcScriptSource)>,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        let scripts: Vec<_> = scripts.collect();
        self.npcs
            .preflight_scripts(scripts.iter().map(|(actor, source)| (*actor, *source)))?;
        self.world
            .validate_npc_admissions(scripts.iter().map(|(actor, _)| *actor))
            .map_err(|_| bace_gameplay_api::NpcFailure::Capacity)
    }
    pub(in crate::kernel) fn admit_npc_script(
        &mut self,
        actor: EntityId,
        source: crate::npc::PreparedNpcScriptSource,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        if self.npcs.admitted_same(actor, source.identity) {
            return Ok(());
        }
        self.register_native_npc_with_properties(
            actor,
            source.program,
            source.use_radius,
            source.properties,
        )?;
        self.npcs.mark_admitted(actor, source.identity)?;
        match self.world.begin_npc_admission(actor) {
            Ok(hold) => {
                self.npcs.set_admission_hold(actor, Some(hold));
                Ok(())
            }
            Err(_) => {
                self.npcs.rollback_admitted(actor)?;
                Err(bace_gameplay_api::NpcFailure::Conflict)
            }
        }
    }
    pub fn npc_admission_pending(&self, actor: EntityId) -> bool {
        self.npcs.admission_pending(actor)
    }
    pub(in crate::kernel) fn rollback_npc_script_admission(
        &mut self,
        actor: EntityId,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        let hold = self.npcs.admission_hold(actor);
        self.npcs.rollback_admitted(actor)?;
        if let Some(hold) = hold
            && self.world.npc_admission_hold(actor).is_some()
        {
            self.world
                .finish_npc_admission(actor, hold)
                .map_err(|_| bace_gameplay_api::NpcFailure::Conflict)?;
        }
        Ok(())
    }
}
