//! One inventory operation freezes the character aggregate until exact adoption.
use super::*;
impl Characters {
    pub(crate) fn reserve_equipment(&mut self, actor: EntityId, operation: u64) -> Result<(), ()> {
        if operation == 0
            || self.reserved(actor)
            || self.get(actor).is_none_or(|p| p.revision() == u64::MAX)
        {
            return Err(());
        }
        self.entries.get_mut(&actor).ok_or(())?.equipment = Some(operation);
        Ok(())
    }
    pub(crate) fn equipment_operation(&self, actor: EntityId) -> Option<u64> {
        self.entries.get(&actor)?.equipment
    }
    pub(crate) fn finish_equipment(&mut self, actor: EntityId, operation: u64, committed: bool) {
        let entry = self
            .entries
            .get_mut(&actor)
            .expect("held equipment character");
        assert_eq!(entry.equipment, Some(operation));
        if committed {
            entry
                .progression
                .touch_revision()
                .expect("preflighted equipment revision");
        }
        entry.equipment = None;
    }
}
