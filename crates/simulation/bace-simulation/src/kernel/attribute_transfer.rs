//! Authenticated AttributeTransferDevice Use and Yes are rechecked against
//! current inventory, equipment and character state. The durable receipt adopts
//! both owners in one simulation turn.
use super::*;
use crate::attribute_transfer::{
    AttributeTransferCommand as C, AttributeTransferDeviceError as E, AttributeTransferResult as R,
    *,
};
use bace_inventory::ItemPlace;

impl Kernel {
    pub fn has_attribute_transfer_work(&self) -> bool {
        !self.attribute_transfers.confirmations.is_empty()
            || !self.attribute_transfers.pending.is_empty()
            || !self.attribute_transfers.outbox.is_empty()
            || !self.attribute_transfers.submitted.is_empty()
    }

    pub(super) fn apply_attribute_transfer_command(
        &mut self,
        command: C,
    ) -> AttributeTransferOutcome {
        let context = match &command {
            C::RequestPrepared { context, .. } | C::Confirm { context, .. } => Some(*context),
            C::Commit { .. } | C::Rollback { .. } => None,
        };
        let result = match command {
            C::RequestPrepared {
                context,
                item,
                revision,
                device,
                activation,
                wielded,
                lifetime,
            } => self
                .request_prepared_attribute_transfer(
                    context, item, revision, device, activation, wielded, lifetime,
                )
                .map(R::Confirmation),
            C::Confirm {
                context,
                token,
                accept,
            } => self
                .confirm_attribute_transfer(context, token, accept)
                .map(R::Proposed),
            C::Commit { receipt } => self
                .confirm_attribute_transfer_committed(&receipt)
                .map(R::Committed),
            C::Rollback { operation } => self
                .reject_attribute_transfer(operation)
                .map(|()| R::RolledBack(operation)),
        };
        AttributeTransferOutcome { context, result }
    }

    pub fn take_attribute_transfer_outcome(&mut self) -> Option<AttributeTransferOutcome> {
        self.attribute_transfer_outcomes.pop_front()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "one frozen authenticated Use carries all equipped source profiles"
    )]
    fn request_prepared_attribute_transfer(
        &mut self,
        context: bace_gameplay_api::ActionContext,
        item: EntityId,
        revision: u64,
        device: PreparedAttributeTransfer,
        activation: Option<bace_inventory::ActivationRequirements>,
        wielded: Vec<(EntityId, u64, bool)>,
        lifetime: u64,
    ) -> Result<AttributeTransferConfirmation, E> {
        if lifetime == 0 || lifetime > 1800 || wielded.len() > 1023 {
            return Err(E::Content);
        }
        self.check_device_busy(context.actor).map_err(|_| E::Busy)?;
        let current = self.inventory.item(item).ok_or(E::Ownership)?;
        if !self.inventory.owned(context.actor, item)
            || !matches!(current.place, ItemPlace::Contained { equipped: 0, .. })
        {
            return Err(E::Ownership);
        }
        if current.revision != revision || self.inventory.reserved(item) {
            return Err(E::Stale);
        }
        let mut ids = std::collections::BTreeSet::new();
        for (id, item_revision, _) in &wielded {
            if !ids.insert(*id)
                || !self.inventory.owned(context.actor, *id)
                || self.inventory.item(*id).is_none_or(|item| {
                    item.revision != *item_revision
                        || !matches!(item.place,ItemPlace::Contained { equipped, .. } if equipped != 0)
                })
                || self.inventory.reserved(*id)
            {
                return Err(E::Stale);
            }
        }
        if self.inventory.items().any(|item| {
            self.inventory.owned(context.actor, item.id)
                && matches!(item.place,ItemPlace::Contained { equipped, .. } if equipped != 0)
                && !ids.contains(&item.id)
        }) {
            return Err(E::MissingWieldProfile);
        }
        let new_wield = wielded
            .iter()
            .filter(|(id, _, _)| !self.attribute_transfers.wielded.contains_key(id))
            .count();
        if self.attribute_transfers.wielded.len() + new_wield > self.attribute_transfers.capacity
            || (!self.attribute_transfers.devices.contains_key(&item)
                && self.attribute_transfers.devices.len() >= self.attribute_transfers.capacity)
        {
            return Err(E::Capacity);
        }
        if let Some(requirements) = &activation {
            self.check_inventory_item_activation(context.actor, requirements)
                .map_err(E::Activation)?;
        }
        self.attribute_transfers
            .confirmations
            .retain(|_, quote| quote.public.expires > self.tick);
        if self
            .attribute_transfers
            .confirmations
            .contains_key(&context.actor)
            || self
                .skill_devices
                .confirmations
                .contains_key(&context.actor)
        {
            return Err(E::Confirmation);
        }
        if self.attribute_transfers.confirmations.len() >= self.attribute_transfers.capacity {
            return Err(E::Capacity);
        }
        let blocked = wielded.iter().any(|(_, _, requirement)| *requirement);
        self.characters
            .get(context.actor)
            .ok_or(E::Ownership)?
            .propose_attribute_transfer(device.from, device.to, blocked)
            .map_err(E::Domain)?;
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|_| E::Ownership)?;
        let token = self
            .attribute_transfers
            .next
            .checked_add(1)
            .filter(|token| *token <= u64::from(u32::MAX))
            .ok_or(E::Capacity)?;
        let quote = AttributeTransferConfirmation {
            token,
            actor: context.actor,
            item,
            expires: self.tick.checked_add(lifetime).ok_or(E::Capacity)?,
            device,
        };
        self.attribute_transfers.next = token;
        self.attribute_transfers.devices.insert(
            item,
            DeviceProfile {
                revision,
                device,
                activation,
            },
        );
        for (id, item_revision, requirement) in wielded {
            self.attribute_transfers
                .wielded
                .insert(id, (item_revision, requirement));
        }
        self.attribute_transfers.confirmations.insert(
            context.actor,
            Confirmation {
                public: quote,
                context,
                revision,
            },
        );
        Ok(quote)
    }

    fn attribute_transfer_profile(
        &self,
        actor: EntityId,
        item: EntityId,
        revision: u64,
        expected: PreparedAttributeTransfer,
    ) -> Result<bool, E> {
        let current = self.inventory.item(item).ok_or(E::Ownership)?;
        if !self.inventory.owned(actor, item)
            || !matches!(current.place, ItemPlace::Contained { equipped: 0, .. })
        {
            return Err(E::Ownership);
        }
        let profile = self
            .attribute_transfers
            .devices
            .get(&item)
            .ok_or(E::Content)?;
        if current.revision != revision
            || profile.revision != revision
            || profile.device != expected
        {
            return Err(E::Stale);
        }
        if let Some(requirements) = &profile.activation {
            self.check_inventory_item_activation(actor, requirements)
                .map_err(E::Activation)?;
        }
        let mut blocked = false;
        for equipped in self.inventory.items().filter(|item| {
            self.inventory.owned(actor, item.id)
                && matches!(item.place,ItemPlace::Contained { equipped, .. } if equipped != 0)
        }) {
            let &(item_revision, requirement) = self
                .attribute_transfers
                .wielded
                .get(&equipped.id)
                .ok_or(E::MissingWieldProfile)?;
            if item_revision != equipped.revision {
                return Err(E::Stale);
            }
            blocked |= requirement;
        }
        Ok(blocked)
    }

    fn confirm_attribute_transfer(
        &mut self,
        context: bace_gameplay_api::ActionContext,
        token: u64,
        accept: bool,
    ) -> Result<Option<AttributeTransferDeviceTicket>, E> {
        let quote = *self
            .attribute_transfers
            .confirmations
            .get(&context.actor)
            .ok_or(E::Confirmation)?;
        if token != quote.public.token
            || quote.context.account != context.account
            || quote.context.session != context.session
        {
            return Err(E::Confirmation);
        }
        if !accept {
            self.characters
                .authorize(context, self.world.body(context.actor).is_ok())
                .map_err(|_| E::Ownership)?;
            self.attribute_transfers
                .confirmations
                .remove(&context.actor);
            return Ok(None);
        }
        if quote.public.expires <= self.tick {
            self.attribute_transfers
                .confirmations
                .remove(&context.actor);
            return Err(E::Expired);
        }
        self.check_device_busy(context.actor).map_err(|_| E::Busy)?;
        if self.attribute_transfers.pending.len() >= self.attribute_transfers.capacity
            || self.attribute_transfers.outbox.len() >= self.attribute_transfers.capacity
        {
            return Err(E::Capacity);
        }
        let blocked = self.attribute_transfer_profile(
            context.actor,
            quote.public.item,
            quote.revision,
            quote.public.device,
        )?;
        self.characters
            .get(context.actor)
            .ok_or(E::Ownership)?
            .propose_attribute_transfer(quote.public.device.from, quote.public.device.to, blocked)
            .map_err(E::Domain)?;
        self.reserve_device_registries(context.actor, quote.public.item, true)
            .map_err(|_| E::Busy)?;
        if self.sync_registry_revisions().is_err() {
            self.reserve_device_registries(context.actor, quote.public.item, false)
                .map_err(|_| E::Busy)?;
            return Err(E::Busy);
        }
        if self
            .attribute_transfer_profile(
                context.actor,
                quote.public.item,
                quote.revision,
                quote.public.device,
            )
            .is_err()
        {
            self.reserve_device_registries(context.actor, quote.public.item, false)
                .map_err(|_| E::Busy)?;
            return Err(E::Stale);
        }
        let character = match self.characters.propose_attribute_transfer(
            context,
            quote.public.device.from,
            quote.public.device.to,
            blocked,
            self.world.body(context.actor).is_ok(),
        ) {
            Ok(ticket) => ticket,
            Err(error) => {
                self.reserve_device_registries(context.actor, quote.public.item, false)
                    .map_err(|_| E::Busy)?;
                return Err(E::Character(error));
            }
        };
        let operation = match self.inventory.take(context.actor, quote.public.item, 1) {
            Ok(operation) => operation,
            Err(error) => {
                self.characters
                    .reject_attribute_transfer(character)
                    .map_err(E::Character)?;
                self.reserve_device_registries(context.actor, quote.public.item, false)
                    .map_err(|_| E::Busy)?;
                return Err(E::Inventory(error));
            }
        };
        if let Err(error) =
            self.reserve_inventory_registries(operation, &[context.actor, quote.public.item])
        {
            self.inventory.reject(operation).map_err(E::Inventory)?;
            self.characters
                .reject_attribute_transfer(character)
                .map_err(E::Character)?;
            self.reserve_device_registries(context.actor, quote.public.item, false)
                .map_err(|_| E::Busy)?;
            return Err(E::Inventory(error));
        }
        let inventory = self
            .inventory
            .pending_ticket(operation)
            .cloned()
            .ok_or(E::Stale)?;
        self.inventory.claim(operation).map_err(E::Inventory)?;
        let ticket = AttributeTransferDeviceTicket {
            character,
            inventory,
        };
        self.attribute_transfers
            .pending
            .insert(operation, ticket.clone());
        self.attribute_transfers.outbox.push_back(operation);
        self.attribute_transfers
            .confirmations
            .remove(&context.actor);
        Ok(Some(ticket))
    }

    pub fn take_attribute_transfer_proposal(&mut self) -> Option<AttributeTransferDeviceTicket> {
        while let Some(operation) = self.attribute_transfers.outbox.pop_front() {
            if let Some(ticket) = self.attribute_transfers.pending.get(&operation) {
                self.attribute_transfers.submitted.insert(operation);
                return Some(ticket.clone());
            }
        }
        None
    }

    /// A lost adapter handoff repeats the same reserved proposal, never a new
    /// character or inventory mutation.
    pub fn retry_attribute_transfer(&mut self, operation: u64) -> Result<(), E> {
        if !self.attribute_transfers.pending.contains_key(&operation) {
            return Err(E::Stale);
        }
        if self.attribute_transfers.outbox.contains(&operation) {
            return Err(E::Busy);
        }
        if self.attribute_transfers.outbox.len() >= self.attribute_transfers.capacity {
            return Err(E::Capacity);
        }
        self.attribute_transfers.submitted.remove(&operation);
        self.attribute_transfers.outbox.push_back(operation);
        Ok(())
    }

    pub fn reject_attribute_transfer(&mut self, operation: u64) -> Result<(), E> {
        let ticket = self
            .attribute_transfers
            .pending
            .get(&operation)
            .ok_or(E::Stale)?
            .clone();
        self.characters
            .validate_attribute_transfer_ticket(ticket.character)
            .map_err(E::Character)?;
        self.preflight_inventory_registries(operation, false)
            .map_err(E::Inventory)?;
        self.characters
            .reject_attribute_transfer(ticket.character)
            .map_err(E::Character)?;
        self.inventory.reject(operation).map_err(E::Inventory)?;
        self.release_inventory_registries(operation)
            .map_err(E::Inventory)?;
        let item = ticket
            .inventory
            .proposal
            .changes
            .first()
            .ok_or(E::Stale)?
            .after
            .id;
        self.reserve_device_registries(ticket.character.context.actor, item, false)
            .map_err(|_| E::Busy)?;
        self.attribute_transfers.submitted.remove(&operation);
        self.attribute_transfers.pending.remove(&operation);
        self.attribute_transfers.outbox.retain(|v| *v != operation);
        Ok(())
    }

    pub fn confirm_attribute_transfer_committed(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<AttributeTransferDeviceTicket, E> {
        if !self
            .attribute_transfers
            .submitted
            .contains(&receipt.operation)
        {
            return Err(E::Stale);
        }
        let ticket = self
            .attribute_transfers
            .pending
            .get(&receipt.operation)
            .ok_or(E::Stale)?
            .clone();
        self.characters
            .validate_attribute_transfer_ticket(ticket.character)
            .map_err(E::Character)?;
        self.inventory
            .validate_receipt(receipt)
            .map_err(E::Inventory)?;
        self.preflight_inventory_registries(receipt.operation, true)
            .map_err(E::Inventory)?;
        self.characters
            .commit_attribute_transfer(ticket.character)
            .map_err(E::Character)?;
        self.inventory.confirm(receipt).map_err(E::Inventory)?;
        let actor = ticket.character.context.actor;
        self.refresh_character_skills(actor)
            .map_err(|_| E::Content)?;
        let item = ticket
            .inventory
            .proposal
            .changes
            .first()
            .ok_or(E::Stale)?
            .after
            .id;
        self.retire_inventory_registries(&ticket.inventory)
            .map_err(E::Inventory)?;
        self.release_inventory_registries(receipt.operation)
            .map_err(E::Inventory)?;
        self.reserve_device_registries(actor, item, false)
            .map_err(|_| E::Busy)?;
        self.attribute_transfers
            .submitted
            .remove(&receipt.operation);
        self.attribute_transfers.pending.remove(&receipt.operation);
        self.attribute_transfers
            .outbox
            .retain(|v| *v != receipt.operation);
        for change in &ticket.inventory.proposal.changes {
            if change.after.place == ItemPlace::Removed {
                self.attribute_transfers.devices.remove(&change.after.id);
                self.attribute_transfers.wielded.remove(&change.after.id);
            } else if let Some(profile) = self.attribute_transfers.devices.get_mut(&change.after.id)
            {
                profile.revision = change.after.revision;
            }
        }
        Ok(ticket)
    }
}
