use super::*;
impl Characters {
    pub(crate) fn crafting_operation(&self, actor: EntityId) -> Option<u64> {
        self.entries.get(&actor)?.crafting
    }
    pub(crate) fn reserve_crafting(&mut self, actor: EntityId, operation: u64) -> Result<(), ()> {
        if operation == 0 || self.reserved(actor) {
            return Err(());
        }
        self.entries.get_mut(&actor).ok_or(())?.crafting = Some(operation);
        Ok(())
    }
    pub(crate) fn release_crafting(&mut self, actor: EntityId, operation: u64) -> Result<(), ()> {
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.crafting.is_some_and(|id| id != operation) {
            return Err(());
        }
        entry.crafting = None;
        Ok(())
    }
    pub(crate) fn adopt_crafting_proficiency(
        &mut self,
        actor: EntityId,
        operation: u64,
        change: &bace_character::ProficiencyChange,
        table: &bace_character::CharacterLevelTable,
    ) -> Result<(), ()> {
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.crafting != Some(operation) {
            return Err(());
        }
        entry
            .progression
            .adopt_proficiency(entry.native_services.as_mut().ok_or(())?, table, change)
            .map_err(|_| ())?;
        entry.crafting = None;
        Ok(())
    }
}
