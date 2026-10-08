//! Claimed component operations keep mana, inventory and exact save snapshots
//! together through retries. Terminal receipts remain until adapter acknowledgment.
use super::*;
use crate::magic_resource_commands::*;
use bace_gameplay_api::{CastRejection, InventoryRejection as E};
use std::sync::Arc;
impl Kernel {
    pub(super) fn handle_magic_resource(
        &mut self,
        command: MagicResourceCommand,
    ) -> Result<(), MagicResourceCommand> {
        if self.magic_resources.outcomes.len() >= self.magic_resources.capacity {
            return Err(command);
        }
        let result = if !command.valid_bounds() {
            Err(E::InvalidState)
        } else {
            self.apply_magic_resource(command.binding, command.action)
        };
        self.magic_resources
            .outcomes
            .push_back(Arc::new(MagicResourceOutcome {
                correlation: command.correlation,
                binding: command.binding,
                result,
            }));
        Ok(())
    }
    fn apply_magic_resource(
        &mut self,
        binding: Option<CharacterBinding>,
        action: MagicResourceAction,
    ) -> Result<MagicResourceResult, E> {
        if let MagicResourceAction::SupplyPortalIds(ids) = &action {
            let (remaining, capacity) = self.supply_portal_ids(ids)?;
            return Ok(MagicResourceResult::PortalIds {
                remaining,
                capacity,
            });
        }
        if let MagicResourceAction::SupplyProjectileIds(ids) = &action {
            if ids.iter().any(|id| {
                self.population.reserves_identity(*id)
                    || self.generator_reserves_identity(*id)
                    || self.combat.reserves_projectile(*id)
                    || self.pets.reserves_identity(*id)
                    || self.inventory.item(*id).is_some()
                    || self.portals.reserves_identity(*id)
            }) {
                return Err(E::InvalidState);
            }
            self.magic
                .preflight_projectile_ids(ids, &self.world)
                .map_err(|e| {
                    if e == CastRejection::Capacity {
                        E::Capacity
                    } else {
                        E::InvalidState
                    }
                })?;
            for id in ids {
                self.magic
                    .supply_projectile_id(*id, &self.world)
                    .expect("complete pool identity preflight");
            }
            let (remaining, capacity) = self.magic.projectile_id_capacity();
            return Ok(MagicResourceResult::ProjectileIds {
                remaining,
                capacity,
            });
        }
        let binding = binding.ok_or(E::NotBound)?;
        match action {
            MagicResourceAction::Inspect { cast } => {
                let operation = self
                    .magic
                    .component_operation(binding.actor, cast)
                    .ok_or(E::DurabilityPending)?;
                if let Some(pending) = self.magic_resources.pending.get(&operation) {
                    if pending.binding != binding {
                        return Err(E::InvalidState);
                    }
                    return Ok(MagicResourceResult::Prepared(pending.ticket.clone()));
                }
                if self.magic_resources.pending.len() >= self.magic_resources.capacity {
                    return Err(E::Capacity);
                }
                let revision = self
                    .characters
                    .get(binding.actor)
                    .ok_or(E::NotBound)?
                    .revision();
                self.magic
                    .rebase_component_snapshot(operation, revision, self.tick as f64 / 30.)
                    .map_err(|_| E::InvalidState)?;
                let resources = self
                    .magic
                    .component_resources(operation)
                    .ok_or(E::InvalidState)?
                    .clone();
                let inventory = self
                    .inventory
                    .pending_ticket(operation)
                    .ok_or(E::InvalidState)?
                    .clone();
                if inventory.actor != binding.actor || inventory.proposal.changes.is_empty() {
                    return Err(E::InvalidState);
                }
                let snapshot = self
                    .read_player_operation_snapshot(
                        binding,
                        crate::PlayerSnapshotOperation::Inventory(operation),
                        resources.before_revision,
                    )
                    .map_err(|_| E::DurabilityPending)?;
                let ticket = Arc::new(PreparedMagicResources {
                    inventory,
                    resources,
                    snapshot: Arc::new(snapshot),
                });
                self.magic_resources.pending.insert(
                    operation,
                    MagicResourcePending {
                        binding,
                        ticket: ticket.clone(),
                        resolved: None,
                    },
                );
                Ok(MagicResourceResult::Prepared(ticket))
            }
            MagicResourceAction::Resolve {
                operation,
                committed,
            } => {
                let pending = self
                    .magic_resources
                    .pending
                    .get(&operation)
                    .ok_or(E::InvalidState)?;
                if pending.binding != binding
                    || pending.resolved.is_some_and(|old| old != committed)
                {
                    return Err(E::InvalidState);
                }
                if pending.resolved.is_none() {
                    if committed {
                        let receipt = crate::InventoryReceipt {
                            operation,
                            revisions: pending
                                .ticket
                                .inventory
                                .proposal
                                .changes
                                .iter()
                                .map(|c| (c.after.id, c.after.revision))
                                .collect(),
                        };
                        self.confirm_inventory_committed_inner(&receipt)?;
                    } else {
                        self.reject_inventory_inner(operation)?;
                    }
                    self.magic_resources
                        .pending
                        .get_mut(&operation)
                        .expect("retained resource receipt")
                        .resolved = Some(committed);
                }
                Ok(MagicResourceResult::Resolved {
                    operation,
                    committed,
                })
            }
            MagicResourceAction::SupplyPortalIds(_)
            | MagicResourceAction::SupplyProjectileIds(_) => unreachable!("handled before binding"),
            MagicResourceAction::Acknowledge { operation } => {
                let pending = self
                    .magic_resources
                    .pending
                    .get(&operation)
                    .ok_or(E::InvalidState)?;
                if pending.binding != binding || pending.resolved.is_none() {
                    return Err(E::DurabilityPending);
                }
                self.magic_resources.pending.remove(&operation);
                Ok(MagicResourceResult::Acknowledged { operation })
            }
        }
    }
    pub(super) fn validate_magic_resource_snapshot(
        &self,
        binding: CharacterBinding,
        operation: u64,
        revision: u64,
    ) -> bool {
        self.magic.component_resources(operation).is_some_and(|r| {
            r.actor == binding.actor
                && r.before_revision == revision
                && self
                    .inventory
                    .pending_ticket(operation)
                    .is_some_and(|t| t.actor == binding.actor)
                && self.inventory.reserved(binding.actor)
        })
    }
    pub(super) fn magic_owns_inventory(&self, operation: u64) -> bool {
        self.magic.component_resources(operation).is_some()
            || self.magic_resources.pending.contains_key(&operation)
    }
    pub fn take_magic_resource_outcome(&mut self) -> Option<Arc<MagicResourceOutcome>> {
        self.magic_resources.outcomes.pop_front()
    }
    pub fn peek_magic_resource_outcome(&self) -> Option<&Arc<MagicResourceOutcome>> {
        self.magic_resources.outcomes.front()
    }
    pub fn has_magic_resource_work(&self) -> bool {
        !self.magic_resources.pending.is_empty()
            || !self.magic_resources.outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::MagicResource(_)))
    }
}

#[cfg(test)]
mod tests;
