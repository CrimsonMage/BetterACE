//! Per-wielder bounded identity index; attack hooks never scan the world's items.
use super::*;
use std::collections::BTreeSet;
fn wielder(item: &InventoryItem) -> Option<EntityId> {
    match item.place {
        ItemPlace::Contained {
            container,
            equipped,
            ..
        } if equipped != 0 => Some(container),
        _ => None,
    }
}
impl Inventory {
    pub(crate) fn equipped_items(&self, actor: EntityId) -> impl Iterator<Item = &InventoryItem> {
        self.equipped
            .get(&actor)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.items.get(id))
    }
    pub(super) fn register_equipment_index(&mut self, item: &InventoryItem) -> Result<(), Error> {
        if let Some(actor) = wielder(item) {
            if self.reserved(actor) {
                return Err(Error::DurabilityPending);
            }
            if self.equipped.get(&actor).is_some_and(|ids| ids.len() >= 64) {
                return Err(Error::Capacity);
            }
            self.equipped.entry(actor).or_default().insert(item.id);
        }
        Ok(())
    }
    pub(super) fn equipment_after(
        &self,
        changes: &[bace_inventory::ItemChange],
    ) -> Result<BTreeMap<EntityId, BTreeSet<EntityId>>, Error> {
        let mut rows = BTreeMap::new();
        for change in changes {
            for actor in change
                .before
                .as_ref()
                .and_then(wielder)
                .into_iter()
                .chain(wielder(&change.after))
            {
                rows.entry(actor)
                    .or_insert_with(|| self.equipped.get(&actor).cloned().unwrap_or_default());
            }
        }
        for change in changes {
            if let Some(actor) = change.before.as_ref().and_then(wielder) {
                rows.get_mut(&actor)
                    .expect("selected wielder")
                    .remove(&change.after.id);
            }
        }
        for change in changes {
            if let Some(actor) = wielder(&change.after) {
                rows.get_mut(&actor)
                    .expect("selected wielder")
                    .insert(change.after.id);
            }
        }
        if rows.values().any(|ids| ids.len() > 64) {
            return Err(Error::Capacity);
        }
        Ok(rows)
    }
}
