use super::*;
impl Characters {
    pub(crate) fn reserve_death(&mut self, actor: EntityId, operation: u64) -> Result<(), ()> {
        if operation == 0 || self.reserved(actor) {
            return Err(());
        }
        self.entries.get_mut(&actor).ok_or(())?.death = Some(operation);
        Ok(())
    }
    pub(crate) fn release_death(&mut self, actor: EntityId, operation: u64) -> Result<(), ()> {
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.death != Some(operation) {
            return Err(());
        }
        entry.death = None;
        Ok(())
    }
    pub(crate) fn adopt_death_revision(
        &mut self,
        actor: EntityId,
        operation: u64,
        before: u64,
        after: u64,
    ) -> Result<(), ()> {
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.death != Some(operation)
            || entry.progression.revision() != before
            || before.checked_add(1) != Some(after)
        {
            return Err(());
        }
        entry.progression.touch_revision().map_err(|_| ())?;
        Ok(())
    }
}
