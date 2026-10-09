//! Constructed acquisition reserves the complete transient graph for one subtype-aware save.
use super::*;
use std::collections::BTreeSet;
impl Inventory {
    pub(crate) fn acquire_constructed(
        &mut self,
        actor: EntityId,
        request: InventoryRequest,
        authority: InventoryAuthority,
    ) -> Result<u64, Error> {
        let InventoryRequest::Move { item: moved, .. } = request else {
            return Err(Error::InvalidState);
        };
        if authority.actor != actor {
            return Err(Error::InvalidState);
        }
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let mut proposal = propose_inventory(
            request,
            authority,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.include_generated_descendants(&mut proposal)?;
        let changed: BTreeSet<_> = proposal
            .changes
            .iter()
            .map(|change| change.after.id)
            .collect();
        let roots: Vec<_> = self
            .constructed
            .iter()
            .copied()
            .filter(|root| changed.contains(root))
            .collect();
        if roots.is_empty() || roots.len() > 128 {
            return Err(Error::InvalidState);
        }
        let mut graph = BTreeSet::new();
        for root in &roots {
            let tree = self.generated_tree(*root)?;
            if tree.iter().any(|id| !changed.contains(id)) {
                return Err(Error::InvalidState);
            }
            graph.extend(tree);
            let root_change = proposal
                .changes
                .iter()
                .find(|change| change.after.id == *root)
                .ok_or(Error::InvalidState)?;
            if self.placement_owner(root_change.after.place, Some(&proposal))? != Some(actor)
                || self.placement_owner(
                    root_change
                        .before
                        .as_ref()
                        .ok_or(Error::InvalidState)?
                        .place,
                    None,
                )? == Some(actor)
            {
                return Err(Error::InvalidState);
            }
        }
        if proposal
            .participants
            .iter()
            .any(|(id, _)| self.constructed_ancestor(*id) && !graph.contains(id))
            || proposal.changes.iter().any(|change| {
                graph.contains(&change.after.id)
                    && change.after.id != moved
                    && change.before.as_ref() != Some(&change.after)
            })
        {
            return Err(Error::InvalidState);
        }
        self.reserve_prepared(actor, proposal, roots)
    }
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
