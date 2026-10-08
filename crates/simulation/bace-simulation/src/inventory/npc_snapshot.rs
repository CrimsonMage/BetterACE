//! NPC source save reservations have a separate domain from region/death holds.
use super::*;
impl Inventory {
    pub(crate) fn npc_descendants(&self, source: EntityId) -> Result<Vec<EntityId>, Error> {
        let mut seen = std::collections::BTreeSet::from([source]);
        for _ in 0..64 {
            let previous = seen.len();
            for item in self.items.values() {
                if matches!(item.place,ItemPlace::Contained{container,..}if seen.contains(&container))
                {
                    seen.insert(item.id);
                }
            }
            if seen.len() > 1025 {
                return Err(Error::Capacity);
            }
            if previous == seen.len() {
                seen.remove(&source);
                return Ok(seen.into_iter().collect());
            }
        }
        Err(Error::InvalidState)
    }
    pub(crate) fn hold_npc_source_items(
        &mut self,
        source: EntityId,
        ticket: u64,
        ids: &[EntityId],
    ) -> Result<(), Error> {
        if ticket == 0 || ids.len() > 1024 || self.npc_holds.len() + ids.len() + 1 > 4096 {
            return Err(Error::Capacity);
        }
        if self.reserved(source)
            || ids
                .iter()
                .any(|id| self.reserved(*id) || !self.items.contains_key(id))
        {
            return Err(Error::DurabilityPending);
        }
        self.npc_holds.insert(source, (source, ticket));
        for &id in ids {
            self.npc_holds.insert(id, (source, ticket));
        }
        Ok(())
    }
    pub(crate) fn release_npc_source_items(&mut self, source: EntityId, ticket: u64) {
        self.npc_holds.retain(|_, owner| *owner != (source, ticket));
    }
    pub(crate) fn adopt_npc_source_items(
        &mut self,
        source: EntityId,
        ticket: u64,
        root: Option<&crate::RegionUnloadItem>,
    ) -> Result<(), Error> {
        if let Some(root) = root {
            let current = self.items.get_mut(&source).ok_or(Error::InvalidState)?;
            if current.revision.checked_add(1) != Some(root.item.revision) {
                return Err(Error::InvalidState);
            }
            *current = root.item.clone();
        }

        for (&id, owner) in &self.npc_holds {
            if *owner == (source, ticket) {
                self.transient.remove(&id);
            }
        }
        Ok(())
    }
}
