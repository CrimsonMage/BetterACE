//! Mutation-invalidated accepted burden. Movement cache hits allocate nothing;
//! the bounded inventory graph is traversed only after that owner's items change.
use super::*;
#[cfg(test)]
mod tests;
impl Inventory {
    pub(crate) fn take_burden_dirty(&mut self) -> Option<EntityId> {
        self.burden_dirty.pop_first()
    }
    pub(super) fn invalidate_burden_owner(&mut self, owner: EntityId) {
        if self.burden.remove(&owner).is_some() {
            self.burden_dirty.insert(owner);
        }
    }
    fn invalidate_all_burdens(&mut self) {
        self.burden_dirty.extend(self.burden.keys().copied());
        self.burden.clear();
    }
    pub(crate) fn actor_burden(&mut self, actor: EntityId) -> Result<u64, Error> {
        if self
            .containers
            .get(&actor)
            .is_none_or(|c| c.root_owner != Some(actor))
        {
            return Err(Error::MissingContainer);
        }
        if let Some(value) = self.burden.get(&actor) {
            return Ok(*value);
        }
        let mut burden = 0u64;
        for item in self.items.values() {
            if self.burden_owner(item.place)? == Some(actor) {
                burden = burden
                    .checked_add(
                        u64::from(item.stack)
                            .checked_mul(u64::from(item.unit_burden))
                            .ok_or(Error::Overflow)?,
                    )
                    .ok_or(Error::Overflow)?;
            }
        }
        self.burden.insert(actor, burden);
        Ok(burden)
    }
    fn burden_owner(&self, mut place: ItemPlace) -> Result<Option<EntityId>, Error> {
        let mut seen = [EntityId(0); 64];
        for depth in 0..64 {
            match place {
                ItemPlace::Contained { container, .. } => {
                    if seen[..depth].contains(&container) {
                        return Err(Error::InvalidState);
                    }
                    seen[depth] = container;
                    if let Some(parent) = self.items.get(&container) {
                        place = parent.place;
                    } else {
                        return self
                            .containers
                            .get(&container)
                            .map(|c| c.root_owner)
                            .ok_or(Error::MissingContainer);
                    }
                }
                ItemPlace::World | ItemPlace::Removed => return Ok(None),
            }
        }
        Err(Error::InvalidState)
    }
    pub(super) fn invalidate_burden_place(&mut self, place: ItemPlace) {
        match self.burden_owner(place) {
            Ok(Some(owner)) => {
                self.invalidate_burden_owner(owner);
            }
            Ok(None) => {}
            Err(_) => self.invalidate_all_burdens(), // A staged graph may still lack its parent.
        }
    }
    pub(super) fn invalidate_burden_proposal(&mut self, proposal: &InventoryProposal) {
        for change in &proposal.changes {
            if let Some(before) = &change.before {
                self.invalidate_burden_place(before.place);
            }
            match self.placement_owner(change.after.place, Some(proposal)) {
                Ok(Some(owner)) => {
                    self.invalidate_burden_owner(owner);
                }
                Ok(None) => {}
                Err(_) => self.invalidate_all_burdens(),
            }
        }
    }
}
