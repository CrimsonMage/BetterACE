//! Inventory and registry reservation/adoption for item XP composites.
use super::*;
use bace_inventory::{InventoryProposal, ItemChange};
impl Kernel {
    pub fn reserve_item_experience(
        &mut self,
        reward: &ItemExperienceReward,
        allow_existing: &[EntityId],
    ) -> Result<Option<crate::InventoryTicket>, E> {
        if &self.prepare_item_experience(reward.actor, reward.amount)? != reward {
            return Err(E::InvalidState);
        }
        if reward.changes.is_empty() {
            return Ok(None);
        }
        if self.item_experience.events.len() + reward.events.len() > self.item_experience.capacity {
            return Err(E::Capacity);
        }
        let root = self
            .inventory
            .container(reward.actor)
            .ok_or(E::MissingContainer)?;
        let mut participants = vec![(root.id, root.revision)];
        for item in self.inventory.equipped_items(reward.actor) {
            participants.push((item.id, item.revision));
        }
        let mut changes = Vec::new();
        for &(id, change) in &reward.changes {
            let before = self.inventory.item(id).ok_or(E::MissingItem)?.clone();
            let mut after = before.clone();
            after.revision = change.after.revision;
            changes.push(ItemChange {
                before: Some(before),
                after,
            });
        }
        for patch in &reward.registries {
            if patch.actor == reward.actor || changes.iter().any(|c| c.after.id == patch.actor) {
                continue;
            }
            let before = self
                .inventory
                .item(patch.actor)
                .ok_or(E::MissingItem)?
                .clone();
            let mut after = before.clone();
            after.revision = before.revision.checked_add(1).ok_or(E::Overflow)?;
            changes.push(ItemChange {
                before: Some(before),
                after,
            });
        }
        let burden = self
            .inventory
            .items()
            .filter(|i| self.inventory.owned(reward.actor, i.id))
            .try_fold(0u64, |sum, item| {
                sum.checked_add(u64::from(item.stack) * u64::from(item.unit_burden))
                    .ok_or(E::Overflow)
            })?;
        let operation = self.inventory.reserve(
            reward.actor,
            InventoryProposal {
                changes,
                participants,
                actor_burden: burden,
                requires_pickup_motion: false,
            },
        )?;
        if let Err(error) = self.reserve_inventory_registries(operation, allow_existing) {
            self.inventory.reject(operation)?;
            return Err(error);
        }
        self.inventory.claim(operation)?;
        Ok(self.inventory.pending_ticket(operation).cloned())
    }
    pub fn validate_item_experience(
        &self,
        reward: &ItemExperienceReward,
        ticket: Option<&crate::InventoryTicket>,
    ) -> Result<(), E> {
        if &self.prepare_item_experience(reward.actor, reward.amount)? != reward {
            return Err(E::InvalidState);
        }
        if reward.changes.is_empty() {
            return if ticket.is_none() {
                Ok(())
            } else {
                Err(E::InvalidState)
            };
        }
        let ticket = ticket.ok_or(E::InvalidState)?;
        if ticket.actor != reward.actor
            || self.inventory.pending_ticket(ticket.operation) != Some(ticket)
        {
            return Err(E::InvalidState);
        }
        self.inventory.validate_receipt(&receipt(ticket))?;
        self.preflight_inventory_registries(ticket.operation, false)?;
        if self.item_experience.events.len() + reward.events.len() > self.item_experience.capacity {
            return Err(E::Capacity);
        }
        self.magic
            .validate_item_experience_registries(&reward.registries)
            .map_err(|_| E::DurabilityPending)
    }
    pub fn adopt_item_experience(
        &mut self,
        reward: &ItemExperienceReward,
        ticket: Option<&crate::InventoryTicket>,
    ) -> Result<(), E> {
        self.validate_item_experience(reward, ticket)?;
        let Some(ticket) = ticket else {
            return Ok(());
        };
        self.magic
            .adopt_item_experience_registries(&reward.registries);
        for patch in &reward.registries {
            self.registry_revisions
                .insert(patch.actor, patch.after_revision);
        }
        self.inventory.confirm(&receipt(ticket))?;
        for &(id, change) in &reward.changes {
            self.item_experience
                .items
                .get_mut(&id)
                .ok_or(E::InvalidState)?
                .experience = Some(change.after);
        }
        self.item_experience
            .events
            .extend(reward.events.iter().cloned());
        self.release_inventory_registries(ticket.operation)
    }
    pub fn reject_item_experience(
        &mut self,
        ticket: Option<&crate::InventoryTicket>,
    ) -> Result<(), E> {
        let Some(ticket) = ticket else {
            return Ok(());
        };
        if self.inventory.pending_ticket(ticket.operation) != Some(ticket) {
            return Err(E::InvalidState);
        }
        self.release_inventory_registries(ticket.operation)?;
        self.inventory.reject(ticket.operation)
    }
}
fn receipt(ticket: &crate::InventoryTicket) -> crate::InventoryReceipt {
    crate::InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    }
}
