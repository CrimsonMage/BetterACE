//! Construction is transient. Valuable transfer waits for a subtype-aware save/restore bridge.
use super::*;
impl Inventory {
    pub(crate) fn constructed_ancestor(&self, mut id: EntityId) -> bool {
        for _ in 0..64 {
            if self.constructed.contains(&id) {
                return true;
            }
            match self.items.get(&id).map(|i| i.place) {
                Some(ItemPlace::Contained { container, .. }) => id = container,
                _ => return false,
            }
        }
        true
    }
    pub(crate) fn mark_constructed(&mut self, root: EntityId) {
        self.constructed.insert(root);
    }
    pub(crate) fn forget_constructed(&mut self, root: EntityId) {
        self.constructed.remove(&root);
    }
    pub(crate) fn preflight_constructed_promotion(&self, root: EntityId) -> Result<(), Error> {
        if !self.constructed.contains(&root) {
            return Err(Error::InvalidState);
        }
        self.generated_tree(root)?;
        let item = self.items.get(&root).ok_or(Error::MissingItem)?;
        let ItemPlace::Contained {
            container,
            slot,
            equipped: 0,
        } = item.place
        else {
            return Err(Error::InvalidState);
        };
        if self.reserved(container)
            || self
                .containers
                .get(&container)
                .is_none_or(|c| c.root_owner.is_some())
        {
            return Err(Error::DurabilityPending);
        }
        // Promotion currently has no durable sibling-compaction companion. It must
        // never change accepted revisions or leave an interior source slot gap.
        if self.items.values().any(|i| i.pack_slot == item.pack_slot && matches!(i.place, ItemPlace::Contained { container: c, slot: s, equipped: 0 } if c == container && s > slot)) {
            return Err(Error::DurabilityPending);
        }
        Ok(())
    }
    pub(crate) fn adopt_constructed_promotion(&mut self, root: EntityId) {
        let item = self
            .items
            .remove(&root)
            .expect("preflighted constructed root");
        self.invalidate_burden_place(item.place);
        self.transient.remove(&root);
        self.constructed.remove(&root);
        // The same root container and children now belong to the admitted NPC.
    }
}
