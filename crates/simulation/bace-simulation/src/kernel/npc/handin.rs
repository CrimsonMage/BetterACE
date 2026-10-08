//! Input consumption and the first scheduled Give row share one durable stage.
use super::Kernel;
use crate::{InventoryReceipt, NpcHandInRequest, NpcHandInTicket};
use bace_gameplay_api::NpcFailure as E;
impl Kernel {
    pub fn validate_npc_handin_snapshot(
        &self,
        operation: u64,
        actor: bace_types::EntityId,
        revision: u64,
    ) -> bool {
        self.npcs
            .handins
            .get(&operation)
            .is_some_and(|(h, adopted)| !adopted && h.request.context.actor == actor)
            && self
                .characters
                .get(actor)
                .is_some_and(|c| c.revision() == revision)
    }
    pub fn prepare_npc_handin(&mut self, request: NpcHandInRequest) -> Result<NpcHandInTicket, E> {
        if self
            .world
            .combatant(request.context.actor)
            .is_none_or(|c| c.health() == 0)
            || self
                .world
                .combatant(request.source)
                .is_some_and(|c| c.health() == 0)
            || self.world.is_in_portal_transit(request.context.actor)
        {
            return Err(E::InvalidInput);
        }
        if self.characters.reserved(request.context.actor)
            || self.inventory.reserved(request.source)
            || self.housing.reserved(request.context.actor)
            || self.npcs.reserved(request.context.actor)
        {
            return Err(E::DurabilityPending);
        }
        if !self.inventory.owned(request.context.actor, request.item) {
            return Err(E::InvalidInput);
        }
        let item = self.inventory.item(request.item).ok_or(E::MissingContent)?;
        if request.count == 0
            || request.count > item.stack
            || item.trade_reserved
            || item.active_pet
        {
            return Err(E::InvalidInput);
        }
        let properties = self
            .world
            .properties(request.source)
            .ok_or(E::MissingContent)?;
        let enabled = |id| {
            matches!(
                properties.get(bace_entity::PropertyFamily::Bool, id),
                Some(bace_entity::PropertyValue::Bool(true))
            )
        };
        let (checkpoint, category) =
            self.npcs
                .preview_handin(&request, item.template, &self.world, self.tick)?;
        let accept_all = enabled(79);
        // Coarse inventory attunement cannot distinguish sticky qualities.
        if accept_all && item.attuned {
            return Err(E::Unsupported);
        }
        if category == 6 && !enabled(8) && !accept_all {
            return Err(E::InvalidInput);
        }
        let accepted_count = if accept_all {
            request.count
        } else if category == 1 {
            0
        } else {
            1
        };
        self.characters
            .authorize(
                request.context,
                self.world.body(request.context.actor).is_ok(),
            )
            .map_err(|_| E::InvalidInput)?;
        let operation = if accepted_count == 0 {
            self.stage_inventory(request.context.actor, |inventory| {
                inventory.inspect_npc_handin(request.context.actor, request.item)
            })
        } else {
            self.propose_item_take(request.context.actor, request.item, accepted_count)
        }
        .map_err(|_| E::Conflict)?;
        let inventory = self
            .inventory
            .pending_ticket(operation)
            .ok_or(E::Conflict)?
            .clone();
        self.inventory.claim(operation).map_err(|_| E::Conflict)?;
        let ticket = NpcHandInTicket {
            category,
            accepted_count,
            character_revision: self
                .characters
                .get(request.context.actor)
                .ok_or(E::MissingActor)?
                .revision(),
            request,
            inventory,
            checkpoint,
        };
        self.npcs.handins.insert(operation, (ticket.clone(), false));
        Ok(ticket)
    }
    pub fn confirm_npc_handin_committed(
        &mut self,
        ticket: &NpcHandInTicket,
        receipt: &InventoryReceipt,
    ) -> Result<(), E> {
        let Some((held, adopted)) = self.npcs.handins.get(&receipt.operation) else {
            return Err(E::Conflict);
        };
        if held != ticket || ticket.inventory.operation != receipt.operation {
            return Err(E::Conflict);
        }
        if !adopted {
            self.inventory
                .validate_receipt(receipt)
                .map_err(|_| E::Conflict)?;
            self.confirm_inventory_committed_inner(receipt)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs
                .handins
                .get_mut(&receipt.operation)
                .ok_or(E::Conflict)?
                .1 = true;
        }
        self.npcs
            .restore_source(ticket.checkpoint.clone(), self.tick)?;
        self.npcs
            .acknowledge_recovery_ready(ticket.request.source, self.tick)?;
        self.npcs.handins.remove(&receipt.operation);
        Ok(())
    }
    pub fn reject_npc_handin(&mut self, ticket: &NpcHandInTicket) -> Result<(), E> {
        if self.npcs.handins.get(&ticket.inventory.operation) != Some(&(ticket.clone(), false)) {
            return Err(E::Conflict);
        }
        self.preflight_inventory_registries(ticket.inventory.operation, false)
            .map_err(|_| E::DurabilityPending)?;
        self.inventory
            .reject(ticket.inventory.operation)
            .map_err(|_| E::Conflict)?;
        self.release_inventory_registries(ticket.inventory.operation)
            .map_err(|_| E::Conflict)?;
        self.npcs.handins.remove(&ticket.inventory.operation);
        Ok(())
    }
}
