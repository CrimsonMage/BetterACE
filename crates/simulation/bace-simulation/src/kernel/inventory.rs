//! Single-owner kernel inventory operations.
use super::*;
impl Kernel {
    pub fn actor_inventory_burden(
        &mut self,
        actor: EntityId,
    ) -> Result<u64, bace_gameplay_api::InventoryRejection> {
        self.inventory.actor_burden(actor)
    }
    pub fn register_inventory_item(
        &mut self,
        item: bace_inventory::InventoryItem,
    ) -> Result<(), bace_gameplay_api::InventoryRejection> {
        if (self.population.reserves_identity(item.id) || self.generator_reserves_identity(item.id))
            || self.magic.reserves_identity(item.id)
        {
            return Err(bace_gameplay_api::InventoryRejection::InvalidState);
        }
        self.inventory.register_item(item)
    }
    pub fn register_inventory_container(
        &mut self,
        c: bace_inventory::InventoryContainer,
    ) -> Result<(), bace_gameplay_api::InventoryRejection> {
        self.inventory.register_container(c)
    }
    pub fn inventory_item(&self, id: EntityId) -> Option<&bace_inventory::InventoryItem> {
        self.inventory.item(id)
    }
    pub fn inventory_container(&self, id: EntityId) -> Option<&bace_inventory::InventoryContainer> {
        self.inventory.container(id)
    }
    pub fn propose_inventory(
        &mut self,
        context: ActionContext,
        request: bace_gameplay_api::InventoryRequest,
        authority: bace_inventory::InventoryAuthority,
    ) -> Result<u64, bace_gameplay_api::InventoryRejection> {
        self.authorize_inventory(context, authority)?;
        self.stage_inventory(context.actor, |inventory| {
            inventory.apply(request, authority)
        })
    }
    /// The adapter prepares the fresh template and completes/revalidates reach
    /// before this method reserves both stacks and all affected ancestors.
    pub fn propose_stack_split(
        &mut self,
        context: ActionContext,
        request: bace_gameplay_api::InventoryRequest,
        authority: bace_inventory::InventoryAuthority,
        prepared: bace_inventory::StackSplitPreparation,
    ) -> Result<u64, bace_gameplay_api::InventoryRejection> {
        if matches!(
            request,
            bace_gameplay_api::InventoryRequest::SplitToWorld { .. }
        ) {
            return Err(bace_gameplay_api::InventoryRejection::MissingGeometry);
        }
        self.validate_split_identity(prepared.fresh.id)?;
        self.authorize_inventory(context, authority)?;
        self.stage_inventory(context.actor, |inventory| {
            inventory.split(request, authority, prepared)
        })
    }
    pub(super) fn authorize_inventory(
        &mut self,
        context: ActionContext,
        authority: bace_inventory::InventoryAuthority,
    ) -> Result<(), bace_gameplay_api::InventoryRejection> {
        use bace_gameplay_api::InventoryRejection as E;
        if authority.actor != context.actor {
            return Err(E::OwnershipMismatch);
        }
        if !self.inventory.can_accept() {
            return Err(E::Capacity);
        }
        if self.npcs.reserved(context.actor)
            || self.characters.reserved(context.actor)
            || self.housing.reserved(context.actor)
        {
            return Err(E::DurabilityPending);
        }
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|e| match e {
                ProgressionActionRejection::NotBound => E::NotBound,
                ProgressionActionRejection::OwnershipMismatch => E::OwnershipMismatch,
                ProgressionActionRejection::StaleSequence => E::StaleSequence,
                _ => E::InvalidState,
            })?;
        Ok(())
    }
    pub fn propose_item_grant(
        &mut self,
        actor: EntityId,
        item: bace_inventory::InventoryItem,
    ) -> Result<u64, bace_gameplay_api::InventoryRejection> {
        if (self.population.reserves_identity(item.id) || self.generator_reserves_identity(item.id))
            || self.magic.reserves_identity(item.id)
            || self.world.contains_identity(item.id)
        {
            return Err(bace_gameplay_api::InventoryRejection::InvalidState);
        }
        self.stage_inventory(actor, |inventory| inventory.grant(actor, item))
    }
    pub fn propose_item_take(
        &mut self,
        actor: EntityId,
        item: EntityId,
        count: u32,
    ) -> Result<u64, bace_gameplay_api::InventoryRejection> {
        self.stage_inventory(actor, |inventory| inventory.take(actor, item, count))
    }
    pub fn propose_items_take(
        &mut self,
        actor: EntityId,
        requested: &[(EntityId, u32)],
    ) -> Result<u64, bace_gameplay_api::InventoryRejection> {
        self.stage_inventory(actor, |inventory| inventory.take_items(actor, requested))
    }
    pub fn propose_template_take(
        &mut self,
        actor: EntityId,
        template: u32,
        count: Option<u32>,
    ) -> Result<u64, bace_gameplay_api::InventoryRejection> {
        self.stage_inventory(actor, |inventory| {
            inventory.take_template(actor, template, count)
        })
    }
    pub fn propose_component_use(
        &mut self,
        actor: EntityId,
        required: &[(u32, u32)],
        consumed: &[(u32, u32)],
    ) -> Result<u64, bace_gameplay_api::InventoryRejection> {
        self.stage_inventory(actor, |inventory| {
            inventory.take_requirements(actor, required, consumed)
        })
    }
    pub fn take_inventory_proposal(&mut self) -> Option<crate::InventoryTicket> {
        self.inventory.take_proposal()
    }
    pub fn retry_inventory(
        &mut self,
        operation: u64,
    ) -> Result<(), bace_gameplay_api::InventoryRejection> {
        if self.npcs.owns_inventory(operation)
            || self.vendor_buy_owns_inventory(operation)
            || self.inventory_commands.owns(operation)
            || self.magic_owns_inventory(operation)
            || self.physical_owns_inventory(operation)
            || self.housing_inventory_bindings.contains_key(&operation)
            || self.skill_devices.pending.contains_key(&operation)
            || self.attribute_transfers.pending.contains_key(&operation)
            || self.crafting.pending(operation).is_some()
            || self.generated_retirements.contains_key(&operation)
            || self.player_deaths.owns_inventory(operation)
            || self.corpse_expiry.owns_inventory(operation)
        {
            return Err(bace_gameplay_api::InventoryRejection::DurabilityPending);
        }
        self.inventory.retry(operation)
    }
    pub fn reject_inventory(
        &mut self,
        operation: u64,
    ) -> Result<(), bace_gameplay_api::InventoryRejection> {
        if self.inventory_commands.owns(operation)
            || self.vendor_buy_owns_inventory(operation)
            || self.physical_owns_inventory(operation)
            || self.magic_owns_inventory(operation)
        {
            return Err(bace_gameplay_api::InventoryRejection::DurabilityPending);
        }
        self.reject_inventory_inner(operation)
    }
    pub(super) fn reject_inventory_inner(
        &mut self,
        operation: u64,
    ) -> Result<(), bace_gameplay_api::InventoryRejection> {
        if self.npcs.owns_inventory(operation)
            || self.housing_inventory_bindings.contains_key(&operation)
            || self.skill_devices.pending.contains_key(&operation)
            || self.attribute_transfers.pending.contains_key(&operation)
            || self.crafting.pending(operation).is_some()
            || self.generated_retirements.contains_key(&operation)
            || self.player_deaths.owns_inventory(operation)
            || self.corpse_expiry.owns_inventory(operation)
        {
            return Err(bace_gameplay_api::InventoryRejection::DurabilityPending);
        }
        self.preflight_inventory_registries(operation, false)?;
        self.inventory.reject(operation)?;
        self.finish_inventory_equipment(operation, false);
        self.finish_inventory_placement(operation, false);
        self.release_inventory_registries(operation)?;
        self.magic.finish_component_operation(operation, false);
        self.world
            .release_vitals(bace_world::VitalReservationToken {
                domain: bace_world::VitalReservationDomain::SpellComponents,
                operation,
            });
        self.finish_pet_operation(operation, false);
        Ok(())
    }
    pub fn confirm_inventory_committed(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<crate::InventoryTicket, bace_gameplay_api::InventoryRejection> {
        if self.inventory_commands.owns(receipt.operation)
            || self.vendor_buy_owns_inventory(receipt.operation)
            || self.physical_owns_inventory(receipt.operation)
            || self.magic_owns_inventory(receipt.operation)
            || self.npcs.owns_inventory(receipt.operation)
            || self.player_deaths.owns_inventory(receipt.operation)
            || self.corpse_expiry.owns_inventory(receipt.operation)
            || self.generated_retirements.contains_key(&receipt.operation)
            || !self
                .inventory
                .generated_changes(receipt.operation)?
                .is_empty()
        {
            return Err(bace_gameplay_api::InventoryRejection::DurabilityPending);
        }
        self.confirm_inventory_committed_inner(receipt)
    }
    pub(super) fn confirm_inventory_committed_inner(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<crate::InventoryTicket, bace_gameplay_api::InventoryRejection> {
        self.confirm_inventory_committed_with_containers(receipt, &[])
    }
    pub(super) fn confirm_inventory_committed_with_containers(
        &mut self,
        receipt: &crate::InventoryReceipt,
        containers: &[bace_inventory::InventoryContainer],
    ) -> Result<crate::InventoryTicket, bace_gameplay_api::InventoryRejection> {
        self.inventory
            .validate_grant_containers(receipt, containers)?;
        if self
            .housing_inventory_bindings
            .contains_key(&receipt.operation)
            || self.skill_devices.pending.contains_key(&receipt.operation)
            || self
                .attribute_transfers
                .pending
                .contains_key(&receipt.operation)
            || self.crafting.pending(receipt.operation).is_some()
            || self.player_deaths.owns_inventory(receipt.operation)
            || self.corpse_expiry.owns_inventory(receipt.operation)
        {
            return Err(bace_gameplay_api::InventoryRejection::DurabilityPending);
        }
        if !self.pet_receipt_ready(receipt.operation) {
            return Err(bace_gameplay_api::InventoryRejection::Capacity);
        }
        self.preflight_inventory_registries(receipt.operation, true)?;
        let magic_actor = self
            .magic
            .component_resources(receipt.operation)
            .map(|r| (r.actor, r.before_revision, r.after_revision));
        if magic_actor.is_some_and(|(actor, before, after)| {
            self.characters.reserved(actor)
                || self
                    .characters
                    .get(actor)
                    .is_none_or(|c| c.revision() < before || c.revision().checked_add(1).is_none())
                || before.checked_add(1) != Some(after)
        }) {
            return Err(bace_gameplay_api::InventoryRejection::DurabilityPending);
        }
        self.inventory.validate_receipt(receipt)?;
        self.magic
            .validate_component_resources(receipt.operation, &self.world)
            .map_err(|e| {
                if e == bace_gameplay_api::CastRejection::Capacity {
                    bace_gameplay_api::InventoryRejection::Capacity
                } else {
                    bace_gameplay_api::InventoryRejection::DurabilityPending
                }
            })?;
        let generators = self.prepare_generated_inventory_transition(receipt)?;
        self.preflight_inventory_placement(receipt.operation)?;
        self.preflight_inventory_equipment(receipt.operation)?;
        let ticket = self.inventory.confirm(receipt)?;
        self.inventory
            .install_grant_containers(ticket.actor, containers);
        self.finish_inventory_equipment(receipt.operation, true);
        self.finish_inventory_placement(receipt.operation, true);
        self.magic
            .adopt_component_resources(receipt.operation, &mut self.world);
        self.retire_inventory_registries(&ticket)?;
        self.release_inventory_registries(receipt.operation)?;
        self.magic
            .finish_component_operation(receipt.operation, true);
        self.world
            .release_vitals(bace_world::VitalReservationToken {
                domain: bace_world::VitalReservationDomain::SpellComponents,
                operation: receipt.operation,
            });
        if let Some((actor, _, _)) = magic_actor {
            self.characters
                .touch_auxiliary(actor)
                .expect("preflighted component aggregate revision");
        }
        self.finish_pet_operation(receipt.operation, true);
        self.adopt_generated_inventory_transition(generators);
        Ok(ticket)
    }
    pub fn has_inventory_state(&self) -> bool {
        self.inventory.has_state()
    }
    pub fn inventory_count(&self, actor: EntityId, template: u32) -> u64 {
        self.inventory.count(actor, template)
    }
    pub fn inventory_free_slots(
        &self,
        actor: EntityId,
        pack: bool,
    ) -> Result<u32, bace_gameplay_api::InventoryRejection> {
        self.inventory.free_slots(actor, pack)
    }
}
