//! Expiry never discards valuable contents ahead of an exact world-epoch save.
mod spill;
use super::*;
use crate::PlayerDeathError as E;
use crate::corpse_expiry::*;
impl Kernel {
    pub fn register_corpse_expiry(
        &mut self,
        corpse: EntityId,
        death_operation: u64,
        expires_tick: u64,
    ) -> Result<(), E> {
        self.validate_corpse_expiry_registration(corpse, death_operation, expires_tick)?;
        self.corpse_expiry.deadlines.insert(
            corpse,
            CorpseDeadline {
                operation: death_operation,
                tick: expires_tick,
            },
        );
        self.population
            .handoff_corpse_expiry(corpse, death_operation);
        Ok(())
    }
    pub(in crate::kernel) fn validate_corpse_expiry_registration(
        &self,
        corpse: EntityId,
        death_operation: u64,
        expires_tick: u64,
    ) -> Result<(), E> {
        if death_operation == 0 || corpse.0 == 0 {
            return Err(E::Invalid);
        }
        if let Some(existing) = self.corpse_expiry.deadlines.get(&corpse) {
            return if existing.operation == death_operation && existing.tick == expires_tick {
                Ok(())
            } else {
                Err(E::Stale)
            };
        }
        if self.corpse_expiry.deadlines.len() >= self.corpse_expiry.capacity {
            return Err(E::Capacity);
        }
        let pending = self.player_deaths.pending.values().any(|p| {
            p.ticket
                .as_ref()
                .is_some_and(|t| t.corpse == corpse && t.operation == death_operation)
        });
        if !pending
            && self
                .world
                .corpse(corpse)
                .is_none_or(|c| c.operation != death_operation)
        {
            return Err(E::Stale);
        }
        Ok(())
    }
    pub fn corpse_expiry_blocked(&self, corpse: EntityId) -> Option<E> {
        self.corpse_expiry.blocked.get(&corpse).copied()
    }
    pub fn has_corpse_expiry_state(&self) -> bool {
        self.corpse_expiry.has_state()
    }
    pub fn peek_corpse_expiry_proposal(&self) -> Option<&CorpseExpiryTicket> {
        self.corpse_expiry
            .pending
            .values()
            .find(|p| !p.submitted)
            .map(|p| &p.ticket)
    }
    pub fn take_corpse_expiry_proposal(&mut self) -> Option<CorpseExpiryTicket> {
        let p = self
            .corpse_expiry
            .pending
            .values_mut()
            .find(|p| !p.submitted)?;
        p.submitted = true;
        Some(p.ticket.clone())
    }
    pub fn peek_corpse_expiry_event(&self) -> Option<&CorpseExpiryEvent> {
        self.corpse_expiry.events.front()
    }
    pub fn take_corpse_expiry_event(&mut self) -> Option<CorpseExpiryEvent> {
        self.corpse_expiry.events.pop_front()
    }
    pub fn retry_corpse_expiry(&mut self, operation: u64) -> Result<(), E> {
        let p = self
            .corpse_expiry
            .pending
            .get_mut(&operation)
            .ok_or(E::Stale)?;
        if !p.submitted {
            return Err(E::Stale);
        }
        p.submitted = false;
        Ok(())
    }
    pub fn confirm_corpse_expiry_committed(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<(), E> {
        let p = self
            .corpse_expiry
            .pending
            .get(&receipt.operation)
            .ok_or(E::Stale)?;
        if !p.submitted || self.corpse_expiry.events.len() >= self.corpse_expiry.capacity {
            return Err(E::Busy);
        }
        self.inventory
            .validate_receipt(receipt)
            .map_err(|_| E::Receipt)?;
        if p.ticket.spill.is_some() {
            self.world
                .validate_corpse_spills(
                    p.ticket.corpse,
                    p.spill.as_deref().ok_or(E::MissingAssets)?,
                )
                .map_err(|_| E::MissingAssets)?;
        }
        let remove_at = self.tick.checked_add(30).ok_or(E::Capacity)?;
        if self.corpse_expiry.retiring.len() >= self.corpse_expiry.capacity {
            return Err(E::Capacity);
        }
        self.preflight_inventory_registries(receipt.operation, true)
            .map_err(|_| E::Busy)?;
        if p.ticket.inventory.proposal.changes.iter().any(|c| {
            self.world.has_reserved_vitals(c.after.id)
                || !self.world.can_retire_actor_motion(c.after.id)
        }) {
            return Err(E::Busy);
        }
        let corpse = p.ticket.corpse;
        let death_operation = p.ticket.death_operation;
        let transient = p.ticket.transient.clone();
        if self.corpse_is_open(corpse) {
            return Err(E::Busy);
        }
        if self
            .world
            .corpse(corpse)
            .is_none_or(|c| c.operation != death_operation)
        {
            return Err(E::Stale);
        }
        let ticket = self.inventory.confirm(receipt).map_err(|_| E::Receipt)?;
        self.inventory.adopt_generated_durability(&transient);
        if let Some(actors) = self
            .corpse_expiry
            .pending
            .get_mut(&receipt.operation)
            .expect("checked expiry")
            .spill
            .take()
        {
            self.world
                .admit_corpse_spills(corpse, actors)
                .map_err(|_| E::Invalid)?;
        }
        self.retire_inventory_registries(&ticket)
            .map_err(|_| E::Busy)?;
        self.release_inventory_registries(receipt.operation)
            .map_err(|_| E::Busy)?;
        for change in ticket.proposal.changes {
            if change.after.place != bace_inventory::ItemPlace::Removed {
                continue;
            }
            if change.after.id != corpse {
                self.world.remove(change.after.id);
            }
            self.corpse_expiry.deadlines.remove(&change.after.id);
            self.corpse_expiry.blocked.remove(&change.after.id);
        }
        self.corpse_expiry.pending.remove(&receipt.operation);
        if self.player_deaths.corpse_access.contains_key(&corpse) {
            self.forget_corpse_access(corpse, death_operation)
                .map_err(|_| E::Stale)?;
        }
        self.corpse_expiry
            .retiring
            .insert(corpse, (death_operation, remove_at));
        self.corpse_expiry.events.push_back(CorpseExpiryEvent {
            corpse,
            death_operation,
            phase: CorpseExpiryPhase::Destroying,
        });
        Ok(())
    }
    pub(in crate::kernel) fn step_corpse_expiries(&mut self) {
        self.step_corpse_removals();
        let mut due = [EntityId(0); 32];
        let mut count = 0;
        for wrapped in [false, true] {
            for (&id, deadline) in &self.corpse_expiry.deadlines {
                if self.corpse_expiry.cursor.is_some_and(|last| id <= last) != wrapped {
                    continue;
                }
                if deadline.tick <= self.tick
                    && self.world.corpse(id).is_some()
                    && !self.corpse_is_open(id)
                    && !self
                        .corpse_expiry
                        .pending
                        .values()
                        .any(|p| p.ticket.corpse == id)
                {
                    due[count] = id;
                    count += 1;
                    if count == due.len() {
                        break;
                    }
                }
            }
            if count == due.len() {
                break;
            }
        }
        if count > 0 {
            self.corpse_expiry.cursor = Some(due[count - 1]);
        }
        for id in due.into_iter().take(count) {
            // Busy means a looter, unload or existing durable operation still owns
            // the tree. The original expiry remains due and is retried unchanged.
            if let Err(error) = self.stage_corpse_expiry(id) {
                self.corpse_expiry.blocked.insert(id, error);
            } else {
                self.corpse_expiry.blocked.remove(&id);
            }
        }
    }
    fn stage_corpse_expiry(&mut self, corpse: EntityId) -> Result<(), E> {
        if self.corpse_expiry.pending.len() >= self.corpse_expiry.capacity {
            return Err(E::Capacity);
        }
        let deadline = *self.corpse_expiry.deadlines.get(&corpse).ok_or(E::Stale)?;
        if self.corpse_is_open(corpse) {
            return Err(E::Busy);
        }
        if self
            .world
            .corpse(corpse)
            .is_none_or(|c| c.operation != deadline.operation)
        {
            return Err(E::Stale);
        }
        if self
            .inventory
            .item(corpse)
            .is_none_or(|i| i.place != bace_inventory::ItemPlace::World)
        {
            return Err(E::MissingAssets);
        }
        self.prepare_inventory_time().map_err(|_| E::Busy)?;
        let mut proposal =
            super::generated_retirement::retirement_proposal(&self.inventory, corpse)
                .map_err(|_| E::Busy)?;
        let spill = self.prepare_corpse_spill_intent(corpse, &mut proposal)?;
        let entry_count: usize = proposal
            .changes
            .iter()
            .filter_map(|c| self.magic.registry(c.after.id))
            .map(|r| r.entries().len())
            .sum();
        if entry_count > 65536 {
            return Err(E::Capacity);
        }
        let enchantments = proposal
            .changes
            .iter()
            .filter_map(|c| {
                self.magic
                    .registry(c.after.id)
                    .map(|r| (c.after.id, r.entries().to_vec()))
            })
            .collect();
        let operation = self
            .inventory
            .reserve(corpse, proposal)
            .map_err(|_| E::Busy)?;
        if self.reserve_inventory_registries(operation, &[]).is_err() {
            self.inventory.reject(operation).map_err(|_| E::Stale)?;
            return Err(E::Busy);
        }
        self.inventory.claim(operation).map_err(|_| E::Stale)?;
        let transient = self
            .inventory
            .generated_changes(operation)
            .map_err(|_| E::Stale)?;
        let inventory = self
            .inventory
            .pending_ticket(operation)
            .ok_or(E::Stale)?
            .clone();
        self.corpse_expiry.pending.insert(
            operation,
            PendingCorpseExpiry {
                ticket: CorpseExpiryTicket {
                    corpse,
                    death_operation: deadline.operation,
                    expires_tick: deadline.tick,
                    inventory,
                    transient,
                    spill,
                    enchantments,
                },
                submitted: false,
                spill: None,
            },
        );
        Ok(())
    }
}
