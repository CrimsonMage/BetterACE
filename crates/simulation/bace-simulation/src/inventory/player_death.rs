//! External parent reservation during bounded cold death preparation. IDs are
//! held in a disjoint namespace from inventory operation numbers.
use super::*;
impl Inventory {
    pub(crate) fn hold_player_death(
        &mut self,
        actor: EntityId,
        operation: u64,
    ) -> Result<(), Error> {
        if operation == 0 || self.reserved(actor) {
            return Err(Error::DurabilityPending);
        }
        let mut ids = vec![actor];
        for item in self.items.values().filter(|i| self.owned(actor, i.id)) {
            if ids.len() > 1024 {
                return Err(Error::Capacity);
            }
            if self.reserved(item.id) {
                return Err(Error::DurabilityPending);
            }
            ids.push(item.id);
        }
        if self.death_holds.len() + ids.len() > 4096 {
            return Err(Error::Capacity);
        }
        for id in ids {
            self.death_holds.insert(id, operation);
        }
        Ok(())
    }
    pub(crate) fn release_player_death_hold(
        &mut self,
        actor: EntityId,
        operation: u64,
    ) -> Result<(), Error> {
        if self.death_holds.get(&actor) != Some(&operation) {
            return Err(Error::InvalidState);
        }
        self.death_holds.retain(|_, op| *op != operation);
        Ok(())
    }
    pub(super) fn death_held_ancestor(&self, item: &InventoryItem) -> bool {
        let mut next = match item.place {
            ItemPlace::Contained { container, .. } => Some(container),
            _ => None,
        };
        for _ in 0..64 {
            let Some(id) = next else {
                return false;
            };
            if self.death_holds.contains_key(&id) || self.region_holds.contains_key(&id) {
                return true;
            }
            next = self.items.get(&id).and_then(|i| match i.place {
                ItemPlace::Contained { container, .. } => Some(container),
                _ => None,
            });
        }
        true
    }
}
