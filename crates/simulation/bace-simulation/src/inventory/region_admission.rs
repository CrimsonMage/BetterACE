//! Validate only the bounded incoming forest, then merge into the sole owner.
use super::*;
pub(crate) struct PreparedRegionInventory(Inventory);
impl Inventory {
    pub(crate) fn prepare_region_admission(
        &self,
        items: &[InventoryItem],
        containers: &[InventoryContainer],
        transient: &std::collections::BTreeSet<EntityId>,
    ) -> Result<PreparedRegionInventory, Error> {
        if items.len() > self.capacity.saturating_sub(self.items.len())
            || containers.len() > self.capacity.saturating_sub(self.containers.len())
        {
            return Err(Error::Capacity);
        }
        if items.iter().any(|i| {
            self.items.contains_key(&i.id)
                || self.containers.contains_key(&i.id)
                || self.reserved(i.id)
        }) || containers.iter().any(|c| {
            self.items.contains_key(&c.id)
                || self.containers.contains_key(&c.id)
                || self.reserved(c.id)
        }) || transient
            .iter()
            .any(|id| !items.iter().any(|i| i.id == *id))
        {
            return Err(Error::InvalidState);
        }
        let mut staged = Inventory::new(4096);
        for container in containers {
            staged.register_container(*container)?;
        }
        for item in items {
            if !matches!(item.place, ItemPlace::World | ItemPlace::Contained { .. }) {
                return Err(Error::InvalidState);
            }
            if item.is_container != containers.iter().any(|c| c.id == item.id)
                || containers
                    .iter()
                    .any(|c| c.id == item.id && c.revision != item.revision)
            {
                return Err(Error::InvalidState);
            }
            staged.register_item(item.clone())?;
        }
        for item in items {
            if let ItemPlace::Contained { container, .. } = item.place {
                bace_inventory::propose_item_changes(
                    container,
                    vec![bace_inventory::ItemChange {
                        before: Some(item.clone()),
                        after: item.clone(),
                    }],
                    InventoryView { items, containers },
                )?;
            }
        }
        staged.transient = transient.clone();
        Ok(PreparedRegionInventory(staged))
    }
    pub(crate) fn adopt_region_admission(&mut self, prepared: PreparedRegionInventory) {
        let staged = prepared.0;
        for container in staged.containers.values() {
            if let Some(owner) = container.root_owner {
                self.invalidate_burden_owner(owner);
            }
        }
        self.items.extend(staged.items);
        self.containers.extend(staged.containers);
        self.equipped.extend(staged.equipped);
        self.transient.extend(staged.transient);
    }
}
impl PreparedRegionInventory {
    pub(crate) fn inventory(&self) -> &Inventory {
        &self.0
    }
}

impl Inventory {
    pub(crate) fn prepare_contained_forest(
        &self,
        roots: &[EntityId],
        items: &[InventoryItem],
        containers: &[InventoryContainer],
    ) -> Result<PreparedRegionInventory, Error> {
        let root_set: std::collections::BTreeSet<_> = roots.iter().copied().collect();
        if root_set.len() != roots.len() || roots.is_empty() || roots.len() > 1024 {
            return Err(Error::InvalidState);
        }
        let mut parents = std::collections::BTreeSet::new();
        for root in roots {
            let original = items
                .iter()
                .find(|i| i.id == *root)
                .ok_or(Error::MissingItem)?;
            let ItemPlace::Contained {
                container: parent,
                equipped: 0,
                ..
            } = original.place
            else {
                return Err(Error::InvalidState);
            };
            if self.reserved(parent) || self.constructed_ancestor(parent) {
                return Err(Error::DurabilityPending);
            }
            if self
                .containers
                .get(&parent)
                .is_none_or(|c| c.root_owner.is_some())
            {
                return Err(Error::InvalidState);
            }
            parents.insert(parent);
        }
        let mut isolated = items.to_vec();
        for item in &mut isolated {
            if root_set.contains(&item.id) {
                item.place = ItemPlace::World;
            } else if matches!(item.place, ItemPlace::World) {
                return Err(Error::InvalidState);
            }
        }
        let transient = items.iter().map(|i| i.id).collect();
        let mut prepared = self.prepare_region_admission(&isolated, containers, &transient)?;
        // Only destination siblings and ancestor metadata are relevant to placement.
        let mut ancestors = parents.clone();
        for parent in &parents {
            let mut id = *parent;
            for depth in 0..64 {
                match self.items.get(&id).map(|i| i.place) {
                    Some(ItemPlace::Contained { container, .. }) => {
                        ancestors.insert(container);
                        id = container;
                    }
                    _ => break,
                }
                if depth == 63 {
                    return Err(Error::InvalidState);
                }
            }
        }
        let views: Vec<_> = self.items.values().filter(|i| ancestors.contains(&i.id) || matches!(i.place, ItemPlace::Contained {container,..} if parents.contains(&container))).cloned().chain(items.iter().cloned()).collect();
        if views.len() > 4096 {
            return Err(Error::Capacity);
        }
        let all_containers: Vec<_> = self
            .containers
            .values()
            .filter(|c| ancestors.contains(&c.id))
            .copied()
            .chain(containers.iter().copied())
            .collect();
        for parent in parents {
            let changes = items.iter().filter(|i| root_set.contains(&i.id) && matches!(i.place, ItemPlace::Contained {container,..} if container == parent)).map(|i| bace_inventory::ItemChange {before:Some(i.clone()),after:i.clone()}).collect();
            bace_inventory::propose_item_changes(
                parent,
                changes,
                InventoryView {
                    items: &views,
                    containers: &all_containers,
                },
            )?;
        }
        for root in roots {
            prepared.0.items.get_mut(root).expect("prepared root").place = items
                .iter()
                .find(|i| i.id == *root)
                .expect("root checked")
                .place;
        }
        Ok(prepared)
    }
}
