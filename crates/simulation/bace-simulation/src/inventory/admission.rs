//! Bounded incoming graph validation; no copy of the live inventory owner.
use super::*;
pub(crate) struct PreparedInventoryAdmission(Inventory);
impl Inventory {
    pub(crate) fn prepare_player_admission(
        &self,
        actor: EntityId,
        items: &[InventoryItem],
        containers: &[InventoryContainer],
    ) -> Result<PreparedInventoryAdmission, Error> {
        if items.len() > 1023
            || containers.len() > 1024
            || self.items.len() + items.len() > self.capacity
            || self.containers.len() + containers.len() > self.capacity
        {
            return Err(Error::Capacity);
        }
        if containers
            .iter()
            .filter(|c| c.id == actor && c.root_owner == Some(actor))
            .count()
            != 1
            || items.iter().any(|i| {
                i.id == actor
                    || self.items.contains_key(&i.id)
                    || self.containers.contains_key(&i.id)
                    || self.reserved(i.id)
            })
            || containers.iter().any(|c| {
                self.containers.contains_key(&c.id)
                    || self.reserved(c.id)
                    || c.root_owner != Some(actor)
            })
        {
            return Err(Error::InvalidState);
        }
        let mut staged = Inventory::new(1024);
        for c in containers {
            if c.id != actor && !items.iter().any(|i| i.id == c.id && i.is_container) {
                return Err(Error::InvalidState);
            }
            staged.register_container(*c)?;
        }
        for i in items {
            if !matches!(i.place, ItemPlace::Contained { .. }) {
                return Err(Error::InvalidState);
            }
            staged.register_item(i.clone())?;
        }
        for i in items {
            if !staged.owned(actor, i.id) {
                return Err(Error::InvalidState);
            }
            bace_inventory::propose_item_changes(
                actor,
                vec![bace_inventory::ItemChange {
                    before: Some(i.clone()),
                    after: i.clone(),
                }],
                InventoryView { items, containers },
            )?;
        }
        Ok(PreparedInventoryAdmission(staged))
    }
    pub(crate) fn adopt_player_admission(&mut self, prepared: PreparedInventoryAdmission) {
        let staged = prepared.0;
        for container in staged.containers.values() {
            if let Some(owner) = container.root_owner {
                self.invalidate_burden_owner(owner);
            }
        }
        self.items.extend(staged.items);
        self.containers.extend(staged.containers);
        self.equipped.extend(staged.equipped);
    }
}
impl PreparedInventoryAdmission {
    pub(crate) fn inventory(&self) -> &Inventory {
        &self.0
    }
}
impl Inventory {
    pub(crate) fn preflight_player_detach(&self, actor: EntityId) -> Result<(), Error> {
        if self.reserved(actor)
            || self
                .containers
                .get(&actor)
                .is_none_or(|c| c.root_owner != Some(actor))
        {
            return Err(Error::InvalidState);
        }
        let ids: Vec<_> = self
            .items
            .values()
            .filter(|i| self.owned(actor, i.id))
            .map(|i| i.id)
            .take(1024)
            .collect();
        if ids.len() > 1023
            || ids
                .iter()
                .any(|id| self.reserved(*id) || self.transient.contains(id))
        {
            return Err(Error::InvalidState);
        }
        if self
            .containers
            .values()
            .any(|c| c.root_owner == Some(actor) && c.id != actor && !ids.contains(&c.id))
        {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
    pub(crate) fn detach_player(
        &mut self,
        actor: EntityId,
    ) -> (Vec<InventoryItem>, Vec<InventoryContainer>) {
        let ids: Vec<_> = self
            .items
            .values()
            .filter(|i| self.owned(actor, i.id))
            .map(|i| i.id)
            .collect();
        let items = ids
            .iter()
            .map(|id| self.items.remove(id).expect("preflighted owned item"))
            .collect();
        let containers = std::iter::once(actor)
            .chain(ids)
            .filter_map(|id| self.containers.remove(&id))
            .collect();
        self.equipped.remove(&actor);
        self.burden.remove(&actor);
        (items, containers)
    }
}
