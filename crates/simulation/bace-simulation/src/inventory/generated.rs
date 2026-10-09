//! Unsaved generated objects share the ordinary inventory owner. A first durable
//! acquisition includes their entire carried tree and reserves every revision.
use super::*;
use std::collections::BTreeSet;
impl Inventory {
    pub(super) fn placement_owner(
        &self,
        mut place: ItemPlace,
        proposal: Option<&InventoryProposal>,
    ) -> Result<Option<EntityId>, Error> {
        let mut seen = BTreeSet::new();
        for _ in 0..64 {
            match place {
                ItemPlace::Contained { container, .. } => {
                    if !seen.insert(container) {
                        return Err(Error::InvalidState);
                    }
                    let parent = proposal
                        .and_then(|p| {
                            p.changes
                                .iter()
                                .find(|c| c.after.id == container)
                                .map(|c| &c.after)
                        })
                        .or_else(|| self.items.get(&container));
                    if let Some(parent) = parent {
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
    pub(crate) fn generated_changes(&self, operation: u64) -> Result<Vec<EntityId>, Error> {
        let ticket = self.pending_ticket(operation).ok_or(Error::InvalidState)?;
        Ok(ticket
            .proposal
            .changes
            .iter()
            .filter(|c| self.transient.contains(&c.after.id))
            .map(|c| c.after.id)
            .collect())
    }
    pub(crate) fn adopt_generated_durability(&mut self, ids: &[EntityId]) {
        for id in ids {
            self.transient.remove(id);
        }
    }
    pub(crate) fn register_generated(
        &mut self,
        items: &[InventoryItem],
        containers: &[InventoryContainer],
    ) -> Result<(), Error> {
        if items.is_empty() || items.len() > 1024 || containers.len() > 1024 {
            return Err(Error::Capacity);
        }
        // Only cold spawn admission clones the bounded owner. No per-tick copy.
        let mut next = self.clone();
        for container in containers {
            next.register_container(*container)?;
        }
        for item in items {
            if !matches!(item.place, ItemPlace::World | ItemPlace::Contained { .. }) {
                return Err(Error::InvalidState);
            }
            next.register_item(item.clone())?;
            next.transient.insert(item.id);
        }
        let views: Vec<_> = next.items.values().cloned().collect();
        let containers: Vec<_> = next.containers.values().copied().collect();
        // The existing proposal validator checks cycles, slots, burden and ancestry
        // without adding a second gameplay validation implementation.
        for item in items {
            let actor = match item.place {
                ItemPlace::Contained { container, .. } => container,
                _ => continue,
            };
            bace_inventory::propose_item_changes(
                actor,
                vec![bace_inventory::ItemChange {
                    before: Some(item.clone()),
                    after: item.clone(),
                }],
                InventoryView {
                    items: &views,
                    containers: &containers,
                },
            )?;
        }
        *self = next;
        Ok(())
    }
    pub(super) fn include_generated_descendants(
        &self,
        proposal: &mut InventoryProposal,
    ) -> Result<(), Error> {
        let mut roots = BTreeSet::new();
        for change in &proposal.changes {
            if let Some(before) = &change.before
                && before.is_container
                && before.place != change.after.place
                && change.after.place != ItemPlace::Removed
                && self.placement_owner(before.place, None)?
                    != self.placement_owner(change.after.place, Some(proposal))?
            {
                roots.insert(change.after.id);
            }
        }
        if roots.is_empty() {
            return Ok(());
        }
        let mut changed: BTreeSet<_> = proposal.changes.iter().map(|c| c.after.id).collect();
        let mut participants: BTreeMap<_, _> = proposal.participants.iter().copied().collect();
        for (id, item) in &self.items {
            if changed.contains(id) {
                continue;
            }
            let mut place = item.place;
            let mut ancestors = Vec::new();
            let mut acquired = false;
            for _ in 0..64 {
                let ItemPlace::Contained { container, .. } = place else {
                    break;
                };
                ancestors.push(container);
                if roots.contains(&container) {
                    acquired = true;
                    break;
                }
                let Some(parent) = self.items.get(&container) else {
                    break;
                };
                place = parent.place;
            }
            if !acquired {
                continue;
            }
            if proposal.changes.len() >= 1024 {
                return Err(Error::Capacity);
            }
            proposal.changes.push(bace_inventory::ItemChange {
                before: Some(item.clone()),
                // Only root ancestry changed. Preserve the accepted child
                // mutation revision while fencing its exact durable snapshot.
                after: item.clone(),
            });
            changed.insert(*id);
            participants.insert(*id, item.revision);
            for ancestor in ancestors {
                let revision = self
                    .items
                    .get(&ancestor)
                    .map(|i| i.revision)
                    .or_else(|| self.containers.get(&ancestor).map(|c| c.revision))
                    .ok_or(Error::MissingContainer)?;
                participants.insert(ancestor, revision);
            }
        }
        if participants.len() > 1024 {
            return Err(Error::Capacity);
        }
        proposal.participants = participants.into_iter().collect();
        Ok(())
    }
}
#[cfg(test)]
mod tests;
impl Inventory {
    pub(crate) fn retire_generated_metadata(&mut self, ids: &[EntityId]) {
        for id in ids {
            self.transient.remove(id);
            if let Some(owner) = self
                .containers
                .get(id)
                .and_then(|container| container.root_owner)
            {
                self.invalidate_burden_owner(owner);
            }
            self.burden.remove(id);
            self.containers.remove(id);
        }
    }
    pub(crate) fn retire_generated_container(&mut self, id: EntityId) -> Result<(), Error> {
        if self.reserved(id)
            || self.items.contains_key(&id)
            || self.items.values().any(
                |item| matches!(item.place,ItemPlace::Contained{container,..} if container==id),
            )
        {
            return Err(Error::DurabilityPending);
        }
        if self
            .containers
            .get(&id)
            .is_some_and(|container| container.root_owner.is_some())
        {
            return Err(Error::InvalidState);
        }
        self.containers.remove(&id);
        Ok(())
    }
    pub(crate) fn generated_tree(&self, root: EntityId) -> Result<Vec<EntityId>, Error> {
        let ids = self.tree_members(root)?;
        if ids
            .iter()
            .any(|id| !self.transient.contains(id) || self.reserved(*id))
        {
            return Err(Error::DurabilityPending);
        }
        Ok(ids)
    }
    pub(crate) fn tree_members(&self, root: EntityId) -> Result<Vec<EntityId>, Error> {
        if !self.items.contains_key(&root) {
            return Err(Error::MissingItem);
        }
        let mut ids = BTreeSet::from([root]);
        for depth in 0..64 {
            let previous = ids.len();
            for item in self.items.values() {
                if let ItemPlace::Contained { container, .. } = item.place
                    && ids.contains(&container)
                {
                    ids.insert(item.id);
                }
            }
            if ids.len() > 1024 {
                return Err(Error::Capacity);
            }
            if ids.len() == previous {
                break;
            }
            if depth == 63 {
                return Err(Error::InvalidState);
            }
        }
        Ok(ids.into_iter().collect())
    }
    pub(crate) fn remove_generated_tree(&mut self, ids: &[EntityId]) -> Result<(), Error> {
        let mut next = self.clone();
        for id in ids {
            let item = next.items.remove(id).ok_or(Error::MissingItem)?;
            next.invalidate_burden_place(item.place);
            next.burden.remove(id);
            next.containers.remove(id);
            next.transient.remove(id);
            if let ItemPlace::Contained {
                container,
                slot,
                equipped: 0,
            } = item.place
            {
                for sibling in next.items.values_mut() {
                    if let ItemPlace::Contained {
                        container: c,
                        slot: at,
                        equipped: 0,
                    } = sibling.place
                        && c == container
                        && at > slot
                        && sibling.pack_slot == item.pack_slot
                    {
                        if !next.transient.contains(&sibling.id)
                            || next.reserved.contains_key(&sibling.id)
                        {
                            return Err(Error::DurabilityPending);
                        }
                        sibling.place = ItemPlace::Contained {
                            container: c,
                            slot: at - 1,
                            equipped: 0,
                        };
                        sibling.revision =
                            sibling.revision.checked_add(1).ok_or(Error::Overflow)?;
                    }
                }
            }
            for members in next.equipped.values_mut() {
                members.remove(id);
            }
        }
        *self = next;
        Ok(())
    }
}
