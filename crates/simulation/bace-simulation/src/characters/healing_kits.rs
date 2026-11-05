//! A kit attempt holds the same character revision as its durable item charge.
use super::*;
impl Characters {
    pub(crate) fn healing_kit_operation(&self, actor: EntityId) -> Option<u64> {
        self.entries.get(&actor)?.healing_kit
    }
    pub(crate) fn reserve_healing_kit(
        &mut self,
        actor: EntityId,
        operation: u64,
    ) -> Result<u64, ()> {
        if operation == 0 || self.reserved(actor) {
            return Err(());
        }
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        let before = entry.progression.revision();
        before.checked_add(1).ok_or(())?;
        entry.healing_kit = Some(operation);
        Ok(before)
    }
    pub(crate) fn validate_healing_kit(
        &self,
        actor: EntityId,
        operation: u64,
        revision: u64,
    ) -> Result<(), ()> {
        let entry = self.entries.get(&actor).ok_or(())?;
        if entry.healing_kit != Some(operation) || entry.progression.revision() != revision {
            return Err(());
        }
        Ok(())
    }
    pub(crate) fn commit_healing_kit(
        &mut self,
        actor: EntityId,
        operation: u64,
        revision: u64,
    ) -> Result<(), ()> {
        self.validate_healing_kit(actor, operation, revision)?;
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        entry.progression.touch_revision().map_err(|_| ())?;
        entry.healing_kit = None;
        Ok(())
    }
    pub(crate) fn reject_healing_kit(
        &mut self,
        actor: EntityId,
        operation: u64,
        revision: u64,
    ) -> Result<(), ()> {
        self.validate_healing_kit(actor, operation, revision)?;
        self.entries.get_mut(&actor).ok_or(())?.healing_kit = None;
        Ok(())
    }
}
