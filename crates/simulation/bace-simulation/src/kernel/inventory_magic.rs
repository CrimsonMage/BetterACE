//! Exact inventory participant registry reservations. No unrelated inventory is
//! frozen, and a failed admission releases only reservations acquired here.
use super::*;
use bace_gameplay_api::InventoryRejection as E;
use bace_inventory::ItemPlace;
impl Kernel {
    pub(super) fn prepare_inventory_time(&mut self) -> Result<(), E> {
        self.magic
            .prepare_registry_time(self.tick as f64 / 30.0)
            .map_err(|_| E::DurabilityPending)?;
        self.sync_registry_revisions().map_err(|_| E::Overflow)
    }
    pub(super) fn stage_inventory<F>(&mut self, actor: EntityId, action: F) -> Result<u64, E>
    where
        F: FnOnce(&mut crate::inventory::Inventory) -> Result<u64, E>,
    {
        if self.equipment_mana_pending(actor)
            || self.characters.reserved(actor)
            || self.npcs.reserved(actor)
            || self.housing.reserved(actor)
        {
            return Err(E::DurabilityPending);
        }
        self.prepare_inventory_time()?;
        let operation = action(&mut self.inventory)?;
        if self.inventory_generator_destruction_pending(operation) {
            self.inventory.reject(operation)?;
            return Err(E::DurabilityPending);
        }
        if let Err(error) = self.reserve_inventory_registries(operation, &[]) {
            self.inventory.reject(operation)?;
            return Err(error);
        }
        Ok(operation)
    }
    /// `allow_existing` is the exact registry set already reserved by the parent
    /// composite, e.g. the housing ticket. Existing reservations are never owned
    /// or released by this map.
    pub(super) fn reserve_inventory_registries(
        &mut self,
        operation: u64,
        allow_existing: &[EntityId],
    ) -> Result<(), E> {
        if self
            .inventory_registry_reservations
            .contains_key(&operation)
        {
            return Err(E::InvalidState);
        }
        let ticket = self
            .inventory
            .pending_ticket(operation)
            .ok_or(E::InvalidState)?;
        let mut ids: Vec<_> = ticket
            .proposal
            .participants
            .iter()
            .map(|p| p.0)
            .chain(std::iter::once(ticket.actor))
            .filter(|id| self.magic.registry(*id).is_some())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids
            .iter()
            .any(|id| self.magic.registry_reserved(*id) && !allow_existing.contains(id))
        {
            return Err(E::DurabilityPending);
        }
        let now = self.tick as f64 / 30.0;
        let mut acquired = Vec::with_capacity(ids.len());
        for id in ids {
            if self.magic.registry_reserved(id) {
                continue;
            }
            if self.magic.reserve_registry(id, true, now).is_err() {
                for previous in acquired {
                    self.magic
                        .reserve_registry(previous, false, now)
                        .map_err(|_| E::DurabilityPending)?;
                }
                return Err(E::DurabilityPending);
            }
            acquired.push(id);
        }
        self.inventory_registry_reservations
            .insert(operation, acquired);
        if let Err(error) = self.preflight_inventory_registries(operation, true) {
            self.release_inventory_registries(operation)?;
            return Err(error);
        }
        Ok(())
    }
    pub(super) fn preflight_inventory_registries(
        &self,
        operation: u64,
        consuming: bool,
    ) -> Result<(), E> {
        let ticket = self
            .inventory
            .pending_ticket(operation)
            .ok_or(E::InvalidState)?;
        let ids = self
            .inventory_registry_reservations
            .get(&operation)
            .ok_or(E::InvalidState)?;
        if ids.iter().any(|id| {
            !self.magic.registry_reserved(*id) || self.magic.registry_failure(*id).is_some()
        }) {
            return Err(E::DurabilityPending);
        }
        if consuming {
            for change in &ticket.proposal.changes {
                if change.after.place == ItemPlace::Removed
                    && self.magic.registry(change.after.id).is_some()
                {
                    self.magic
                        .can_retire_item_registry(change.after.id, self.tick as f64 / 30.0)
                        .map_err(|_| E::DurabilityPending)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn release_inventory_registries(&mut self, operation: u64) -> Result<(), E> {
        let ids = self
            .inventory_registry_reservations
            .get(&operation)
            .ok_or(E::InvalidState)?;
        for &id in ids {
            if self.magic.registry(id).is_some() {
                self.magic
                    .reserve_registry(id, false, self.tick as f64 / 30.0)
                    .map_err(|_| E::DurabilityPending)?;
            }
        }
        self.inventory_registry_reservations.remove(&operation);
        Ok(())
    }
    /// Destruction is authorized only by the already-committed item tombstone.
    pub(super) fn retire_inventory_registries(
        &mut self,
        ticket: &crate::InventoryTicket,
    ) -> Result<(), E> {
        for change in &ticket.proposal.changes {
            if change.after.place == ItemPlace::Removed
                && self.magic.registry(change.after.id).is_some()
            {
                self.magic
                    .retire_item_registry(change.after.id, self.tick as f64 / 30.0)
                    .map_err(|_| E::DurabilityPending)?;
                self.registry_revisions.remove(&change.after.id);
            }
        }
        Ok(())
    }
}
