//! A disjoint save-before-eviction reservation; no accepted item revision changes.
use super::*;
impl Inventory {
    pub(crate) fn item_transient(&self, id: EntityId) -> bool {
        self.transient.contains(&id)
    }
    pub(crate) fn world_tree(&self, root: EntityId) -> Result<Vec<EntityId>, Error> {
        if self
            .items
            .get(&root)
            .is_none_or(|i| i.place != ItemPlace::World)
        {
            return Err(Error::InvalidState);
        }
        let mut ids = std::collections::BTreeSet::from([root]);
        for _ in 0..64 {
            let before = ids.len();
            for item in self.items.values() {
                if matches!(item.place,ItemPlace::Contained{container,..} if ids.contains(&container))
                {
                    ids.insert(item.id);
                }
            }
            if ids.len() > 4096 {
                return Err(Error::Capacity);
            }
            if before == ids.len() {
                return Ok(ids.into_iter().collect());
            }
        }
        Err(Error::InvalidState)
    }
    pub(crate) fn hold_region_items(
        &mut self,
        ids: &[EntityId],
        operation: u64,
    ) -> Result<(), Error> {
        if operation == 0 || ids.len() > 4096 || self.region_holds.len() + ids.len() > 4096 {
            return Err(Error::Capacity);
        }
        if ids
            .iter()
            .any(|id| self.reserved(*id) || !self.items.contains_key(id))
        {
            return Err(Error::DurabilityPending);
        }
        for &id in ids {
            self.region_holds.insert(id, operation);
        }
        Ok(())
    }
    pub(crate) fn release_region_items(&mut self, operation: u64) {
        self.region_holds.retain(|_, op| *op != operation);
    }
    pub(crate) fn evict_region_items(
        &mut self,
        ids: &[(EntityId, u64)],
        operation: u64,
    ) -> Result<(), Error> {
        if ids.iter().any(|(id, rev)| {
            self.region_holds.get(id) != Some(&operation)
                || self.items.get(id).is_none_or(|i| i.revision != *rev)
        }) {
            return Err(Error::DurabilityPending);
        }
        for &(id, _) in ids {
            if let Some(item) = self.items.get(&id) {
                self.invalidate_burden_place(item.place);
            }
            self.burden.remove(&id);
            self.items.remove(&id);
            self.containers.remove(&id);
            self.transient.remove(&id);
            self.equipped.remove(&id);
        }
        self.release_region_items(operation);
        Ok(())
    }
}
impl Inventory {
    pub(crate) fn preflight_transient_region_tree(&self, ids: &[EntityId]) -> Result<(), Error> {
        if ids.is_empty() || ids.len() > 1024 {
            return Err(Error::Capacity);
        }
        let set: std::collections::BTreeSet<_> = ids.iter().copied().collect();
        if set.len()!=ids.len()||self.items.values().any(|i|matches!(i.place,ItemPlace::Contained{container,..} if set.contains(&container)&&!set.contains(&i.id))){return Err(Error::InvalidState);}
        for &id in ids {
            let i = self.items.get(&id).ok_or(Error::MissingItem)?;
            if !self.transient.contains(&id)
                || self.reserved(id)
                || !match i.place {
                    ItemPlace::World => true,
                    ItemPlace::Contained { container, .. } => set.contains(&container),
                    _ => false,
                }
            {
                return Err(Error::DurabilityPending);
            }
        }
        Ok(())
    }
    pub(crate) fn adopt_transient_region_tree(&mut self, ids: &[EntityId]) {
        for id in ids {
            if let Some(item) = self.items.get(id) {
                self.invalidate_burden_place(item.place);
            }
            self.burden.remove(id);
            self.items.remove(id);
            self.containers.remove(id);
            self.transient.remove(id);
            self.equipped.remove(id);
        }
        for members in self.equipped.values_mut() {
            for id in ids {
                members.remove(id);
            }
        }
        self.equipped.retain(|_, members| !members.is_empty());
    }
}
