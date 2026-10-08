//! Exact ammunition reservation, snapshot, durable adoption and launch fencing.
use super::*;
use crate::physical_resources::*;
use bace_gameplay_api::{InventoryRejection as E, weapon_combat::PhysicalLaunchReceipt};
impl Kernel {
    pub(super) fn handle_physical_resource(&mut self, command: PhysicalResourceCommand) {
        let correlation = command.correlation;
        let result = if command.valid_bounds() {
            self.apply_physical_resource(command.action)
        } else {
            Err(E::InvalidState)
        };
        self.physical_resource_outcomes
            .push_back(std::sync::Arc::new(PhysicalResourceOutcome {
                correlation,
                result,
            }));
    }
    fn apply_physical_resource(
        &mut self,
        action: PhysicalResourceAction,
    ) -> Result<PhysicalResourceDecision, E> {
        match action {
            PhysicalResourceAction::Abort { binding, operation } => {
                if self.physical_resources.contains_key(&operation) {
                    return Err(E::DurabilityPending);
                }
                match self.characters.can_take_complete(binding) {
                    Ok(()) | Err(CharacterRegistrationError::DurabilityPending) => {}
                    _ => return Err(E::OwnershipMismatch),
                }
                if self
                    .combat
                    .pending_launch(operation)
                    .is_none_or(|p| p.actor != binding.actor.0)
                {
                    return Err(E::InvalidState);
                }
                self.combat
                    .reject_launch(operation, &mut self.world)
                    .map_err(|_| E::DurabilityPending)?;
                Ok(PhysicalResourceDecision::Aborted { operation })
            }
            PhysicalResourceAction::Prepare { binding, launch } => self
                .prepare_physical_resource(binding, *launch)
                .map(PhysicalResourceDecision::Prepared),
            PhysicalResourceAction::Resolve {
                operation,
                committed,
            } => self.resolve_physical_resource(operation, committed),
            PhysicalResourceAction::Acknowledge { operation } => {
                let pending = self
                    .physical_resources
                    .get(&operation)
                    .ok_or(E::InvalidState)?;
                if !matches!(
                    pending.phase,
                    PhysicalResourcePhase::Launched | PhysicalResourcePhase::Rejected
                ) {
                    return Err(E::DurabilityPending);
                }
                self.physical_resources.remove(&operation);
                Ok(PhysicalResourceDecision::Acknowledged { operation })
            }
            PhysicalResourceAction::SupplyProjectiles(ids) => {
                // All identities preflight together; rejected input keeps its
                // allocator reservation in the runtime for an exact retry.
                self.combat
                    .preflight_missile_ids(&ids, &self.world)
                    .map_err(|_| E::Capacity)?;
                if ids.iter().any(|id| {
                    self.population.reserves_identity(*id)
                        || self.generator_reserves_identity(*id)
                        || self.magic.reserves_identity(*id)
                        || self.inventory.item(*id).is_some()
                }) {
                    return Err(E::InvalidState);
                }
                for id in ids {
                    self.combat
                        .supply_missile_id(id, &self.world)
                        .expect("same-owner identity preflight");
                }
                Ok(PhysicalResourceDecision::Projectiles {
                    remaining: self.combat.available_missile_ids(),
                })
            }
        }
    }
    fn prepare_physical_resource(
        &mut self,
        binding: CharacterBinding,
        launch: bace_gameplay_api::weapon_combat::PhysicalLaunchProposal,
    ) -> Result<std::sync::Arc<PhysicalResourceTicket>, E> {
        if let Some(pending) = self.physical_resources.get(&launch.operation) {
            if pending.binding != binding || pending.launch != launch {
                return Err(E::InvalidState);
            }
            return pending.ticket.clone().ok_or(E::DurabilityPending);
        }
        if self.physical_resources.len() >= 8 {
            return Err(E::Capacity);
        }
        self.characters
            .can_take_complete(binding)
            .map_err(|_| E::DurabilityPending)?;
        if self.combat.pending_launch(launch.operation) != Some(&launch) || launch.consumed != 1 {
            return Err(E::InvalidState);
        }
        self.prepare_inventory_time()?;
        let ammo = self
            .inventory
            .item(EntityId(launch.ammunition))
            .ok_or(E::MissingItem)?;
        if ammo.revision != launch.ammunition_revision
            || ammo.stack != launch.expected_count
            || !self.inventory.owned(binding.actor, ammo.id)
        {
            return Err(E::InvalidState);
        }
        let operation = self.propose_item_take(binding.actor, ammo.id, launch.consumed)?;
        let inventory = self
            .inventory
            .pending_ticket(operation)
            .expect("created inventory reservation")
            .clone();
        let actor_revision = self
            .characters
            .get(binding.actor)
            .ok_or(E::NotBound)?
            .revision();
        self.physical_resources.insert(
            launch.operation,
            PhysicalResourcePending {
                binding,
                launch: launch.clone(),
                inventory: inventory.clone(),
                actor_revision,
                ticket: None,
                phase: PhysicalResourcePhase::Reserved,
            },
        );
        let snapshot = match self.read_player_operation_snapshot(
            binding,
            crate::PlayerSnapshotOperation::PhysicalAmmo(launch.operation),
            actor_revision,
        ) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                self.physical_resources.remove(&launch.operation);
                self.reject_inventory_inner(operation)?;
                return Err(E::DurabilityPending);
            }
        };
        self.inventory.claim(operation)?;
        let ticket = std::sync::Arc::new(PhysicalResourceTicket {
            appearance: self.combat.pending_launch_appearance(launch.operation),
            binding,
            launch: launch.clone(),
            inventory,
            actor_revision,
            snapshot: std::sync::Arc::new(snapshot),
        });
        self.physical_resources
            .get_mut(&launch.operation)
            .expect("retained reservation")
            .ticket = Some(ticket.clone());
        Ok(ticket)
    }
    fn resolve_physical_resource(
        &mut self,
        operation: u64,
        committed: bool,
    ) -> Result<PhysicalResourceDecision, E> {
        let pending = self
            .physical_resources
            .get(&operation)
            .ok_or(E::InvalidState)?;
        let phase = pending.phase;
        if (!committed
            && matches!(
                phase,
                PhysicalResourcePhase::InventoryCommitted | PhysicalResourcePhase::Launched
            ))
            || (committed && phase == PhysicalResourcePhase::Rejected)
        {
            return Err(E::InvalidState);
        }
        if !committed && phase == PhysicalResourcePhase::Reserved {
            let inventory = pending.inventory.operation;
            self.preflight_inventory_registries(inventory, false)?;
            self.combat
                .reject_launch(operation, &mut self.world)
                .map_err(|_| E::InvalidState)?;
            self.reject_inventory_inner(inventory)?;
            self.physical_resources
                .get_mut(&operation)
                .expect("retained stage")
                .phase = PhysicalResourcePhase::Rejected;
        } else if committed && phase == PhysicalResourcePhase::Reserved {
            let receipt = crate::InventoryReceipt {
                operation: pending.inventory.operation,
                revisions: pending
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .map(|change| (change.after.id, change.after.revision))
                    .collect(),
            };
            self.confirm_inventory_committed_inner(&receipt)?;
            self.physical_resources
                .get_mut(&operation)
                .expect("retained stage")
                .phase = PhysicalResourcePhase::InventoryCommitted;
        }
        if committed
            && self.physical_resources[&operation].phase
                == PhysicalResourcePhase::InventoryCommitted
        {
            let launch = &self.physical_resources[&operation].launch;
            let receipt = PhysicalLaunchReceipt {
                operation,
                ammunition: launch.ammunition,
                before_revision: launch.ammunition_revision,
                after_revision: launch
                    .ammunition_revision
                    .checked_add(1)
                    .ok_or(E::Overflow)?,
                remaining: launch.expected_count - launch.consumed,
            };
            self.combat
                .confirm_launch_with_observers(receipt, &mut self.world, &self.characters)
                .map_err(|_| E::DurabilityPending)?;
            self.physical_resources
                .get_mut(&operation)
                .expect("retained stage")
                .phase = PhysicalResourcePhase::Launched;
        }
        Ok(PhysicalResourceDecision::Resolved {
            operation,
            committed,
        })
    }
    pub fn peek_physical_resource_outcome(
        &self,
    ) -> Option<&std::sync::Arc<PhysicalResourceOutcome>> {
        self.physical_resource_outcomes.front()
    }
    pub fn take_physical_resource_outcome(
        &mut self,
    ) -> Option<std::sync::Arc<PhysicalResourceOutcome>> {
        self.physical_resource_outcomes.pop_front()
    }
    pub fn has_physical_resource_work(&self) -> bool {
        !self.physical_resources.is_empty()
            || !self.physical_resource_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|command| matches!(command, Command::PhysicalResource(_)))
    }
    pub(super) fn physical_owns_inventory(&self, operation: u64) -> bool {
        self.physical_resources
            .values()
            .any(|p| p.inventory.operation == operation)
    }
    pub(super) fn validate_physical_resource_snapshot(
        &self,
        operation: u64,
        binding: CharacterBinding,
        revision: u64,
    ) -> bool {
        self.physical_resources.get(&operation).is_some_and(|p| {
            p.binding == binding
                && p.actor_revision == revision
                && p.phase == PhysicalResourcePhase::Reserved
                && self.inventory.reserved(binding.actor)
        })
    }
}
