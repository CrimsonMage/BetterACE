//! Native Give/Take rows use the existing inventory owner and one joint durable
//! workflow stage. Detached Give timing is established before item preparation.
use super::Kernel;
use crate::{InventoryReceipt, NpcEffect, NpcProposal, npc::NpcInventoryTicket};
use bace_gameplay_api::{NpcCompletion, NpcFailure as E, NpcOperation};
use bace_inventory::InventoryItem;

impl Kernel {
    /// `prepared` is a fresh content-owned item for Give, including the authored
    /// palette/shade. Take never accepts a caller-selected replacement item.
    /// Give's source rotation/detachment must already have completed admission.
    pub fn prepare_npc_inventory(
        &mut self,
        expected: &NpcProposal,
        prepared: Option<InventoryItem>,
    ) -> Result<NpcInventoryTicket, E> {
        self.prepare_npc_inventory_batch(expected, prepared.into_iter().collect(), vec![])
    }
    pub fn prepare_npc_inventory_batch(
        &mut self,
        expected: &NpcProposal,
        prepared: Vec<InventoryItem>,
        containers: Vec<bace_inventory::InventoryContainer>,
    ) -> Result<NpcInventoryTicket, E> {
        self.npcs.validate_service(expected)?;
        if self
            .npcs
            .inventory_services
            .values()
            .any(|t| t.npc.ticket == expected.ticket)
        {
            return Err(E::Conflict);
        }
        let actor = expected.context.target.ok_or(E::MissingActor)?;
        let character_revision = self
            .characters
            .get(actor)
            .ok_or(E::MissingActor)?
            .revision();
        if self.characters.reserved(actor)
            || self.housing.reserved(actor)
            || self.npcs.reserved_except(actor, expected.ticket)
        {
            return Err(E::DurabilityPending);
        }
        self.prepare_inventory_time()
            .map_err(|_| E::DurabilityPending)?;
        let operation = match &expected.effect {
            NpcEffect::Service(NpcOperation::Give {
                template, count, ..
            }) => {
                if !self.npcs.service_detached(expected.ticket) {
                    return Err(E::DurabilityPending);
                }
                if prepared.is_empty()
                    || prepared.len() > 1024
                    || prepared
                        .iter()
                        .try_fold(0u32, |n, i| n.checked_add(i.stack))
                        != Some(*count)
                    || prepared.iter().any(|item| {
                        item.template != *template
                            || self.world.contains_identity(item.id)
                            || self.population.reserves_identity(item.id)
                            || self.generator_reserves_identity(item.id)
                            || self.magic.reserves_identity(item.id)
                    })
                    || containers.len() != prepared.iter().filter(|i| i.is_container).count()
                    || containers
                        .iter()
                        .any(|c| !prepared.iter().any(|i| i.id == c.id && i.is_container))
                {
                    return Err(E::InvalidInput);
                }
                self.inventory.grant_many(actor, &prepared)
            }
            NpcEffect::Service(NpcOperation::Take { template, count })
                if prepared.is_empty() && containers.is_empty() =>
            {
                self.inventory.take_template(actor, *template, *count)
            }
            _ => return Err(E::Unsupported),
        }
        .map_err(|_| E::Conflict)?;
        if self.inventory_generator_destruction_pending(operation) {
            self.inventory.reject(operation).map_err(|_| E::Conflict)?;
            return Err(E::DurabilityPending);
        }
        if self.reserve_inventory_registries(operation, &[]).is_err() {
            self.inventory.reject(operation).map_err(|_| E::Conflict)?;
            return Err(E::DurabilityPending);
        }
        let inventory = self
            .inventory
            .pending_ticket(operation)
            .ok_or(E::Conflict)?
            .clone();
        let receipt = InventoryReceipt {
            operation,
            revisions: inventory
                .proposal
                .changes
                .iter()
                .map(|c| (c.after.id, c.after.revision))
                .collect(),
        };
        if self
            .inventory
            .validate_grant_containers(&receipt, &containers)
            .is_err()
        {
            self.inventory.reject(operation).map_err(|_| E::Conflict)?;
            self.release_inventory_registries(operation)
                .map_err(|_| E::Conflict)?;
            return Err(E::InvalidInput);
        }
        self.inventory.claim(operation).map_err(|_| E::Conflict)?;
        let ticket = NpcInventoryTicket {
            containers,
            character_revision,
            npc: expected.clone(),
            inventory,
        };
        self.npcs
            .inventory_services
            .insert(operation, ticket.clone());
        Ok(ticket)
    }

    /// The caller supplies the receipt from the atomic inventory+NPC checkpoint
    /// operation, never a standalone inventory write or queue-admission result.
    pub fn confirm_npc_inventory_committed(
        &mut self,
        ticket: &NpcInventoryTicket,
        receipt: &InventoryReceipt,
    ) -> Result<(), E> {
        if self.npcs.inventory_services.get(&receipt.operation) != Some(ticket)
            || ticket.inventory.operation != receipt.operation
        {
            return Err(E::Conflict);
        }
        self.npcs.validate_service(&ticket.npc)?;
        self.inventory
            .validate_receipt(receipt)
            .map_err(|_| E::Conflict)?;
        self.confirm_inventory_committed_with_containers(receipt, &ticket.containers)
            .map_err(|_| E::DurabilityPending)?;
        // Record adoption before resuming any row: capacity or a later owner
        // error must never cause the inventory effect to run twice.
        self.npcs
            .mark_service_adopted(&ticket.npc, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.npcs.inventory_services.remove(&receipt.operation);
        self.confirm_npc_committed(&ticket.npc)
    }

    /// Only a definitely rolled-back joint operation permits releasing its item
    /// reservation. The authored row remains pending for an explicit retry.
    pub fn reject_npc_inventory(&mut self, ticket: &NpcInventoryTicket) -> Result<(), E> {
        let operation = ticket.inventory.operation;
        if self.npcs.inventory_services.get(&operation) != Some(ticket) {
            return Err(E::Conflict);
        }
        self.preflight_inventory_registries(operation, false)
            .map_err(|_| E::DurabilityPending)?;
        self.inventory.reject(operation).map_err(|_| E::Conflict)?;
        self.release_inventory_registries(operation)
            .map_err(|_| E::Conflict)?;
        self.npcs.inventory_services.remove(&operation);
        Ok(())
    }
}
