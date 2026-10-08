//! Distinct gag reservation never aliases another valuable-operation domain.
use super::*;
impl Characters {
    pub(crate) fn reserve_gag(
        &mut self,
        actor: EntityId,
        operation: u64,
        revision: u64,
    ) -> Result<(), ()> {
        if operation == 0 || self.reserved(actor) {
            return Err(());
        }
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.progression.revision() != revision || revision == u64::MAX {
            return Err(());
        }
        entry.gag = Some(operation);
        Ok(())
    }
    pub(crate) fn gag_operation(&self, actor: EntityId) -> Option<u64> {
        self.entries.get(&actor)?.gag
    }
    pub(crate) fn finish_gag(
        &mut self,
        actor: EntityId,
        operation: u64,
        revision: u64,
        commit: bool,
    ) -> Result<(), ()> {
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.gag != Some(operation) || entry.progression.revision() != revision {
            return Err(());
        }
        if commit {
            entry.progression.touch_revision().map_err(|_| ())?;
        }
        entry.gag = None;
        Ok(())
    }
}
