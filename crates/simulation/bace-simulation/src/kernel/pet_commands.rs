//! Pet command admission uses the same character serial authority as other Use.
use super::*;
use crate::{PetAction, PetCommand, PetDecision, PetError, PetOutcome};

impl Kernel {
    pub fn take_pet_outcome(&mut self) -> Option<PetOutcome> {
        self.pet_outcomes.pop_front()
    }

    pub fn has_pet_command_state(&self) -> bool {
        !self.pet_outcomes.is_empty() || self.commands.iter().any(|c| matches!(c, Command::Pet(_)))
    }

    pub(super) fn handle_pet_command(&mut self, command: PetCommand) {
        let result = if !command.valid_bounds() {
            Err(PetError::InvalidProfile)
        } else {
            match command.action {
                PetAction::RegisterOwner { owner, user } => self
                    .register_pet_owner(owner, user)
                    .map(|()| PetDecision::Registered),
                PetAction::Summon {
                    context,
                    device,
                    device_revision,
                    pet,
                    user,
                    activation,
                    profile,
                } => self
                    .characters
                    .authorize(context, self.world.body(context.actor).is_ok())
                    .map_err(|_| PetError::Unauthorized)
                    .and_then(|_| {
                        if self
                            .inventory
                            .item(device)
                            .is_none_or(|item| item.revision != device_revision)
                        {
                            return Err(PetError::MissingDevice);
                        }
                        if let Some(requirements) = &activation {
                            self.check_inventory_item_activation(context.actor, requirements)
                                .map_err(PetError::Activation)?;
                        }
                        self.register_pet_owner(context.actor, user)?;
                        let operation =
                            self.propose_combat_pet(context.actor, device, pet, profile)?;
                        let (_, registry_revision, registry_after) = self
                            .pending_pet_registry(operation)
                            .ok_or(PetError::UnknownOperation)?;
                        let registry_after = registry_after.to_vec();
                        let ticket = self
                            .inventory
                            .pending_ticket(operation)
                            .cloned()
                            .ok_or(PetError::UnknownOperation)?;
                        let actor_revision = self
                            .characters
                            .get(context.actor)
                            .ok_or(PetError::MissingOwner)?
                            .revision();
                        self.inventory
                            .claim(operation)
                            .map_err(|_| PetError::Durability)?;
                        Ok(PetDecision::Proposed {
                            ticket,
                            actor_revision,
                            registry_revision,
                            registry_after,
                        })
                    }),
                PetAction::SummonPassive {
                    context,
                    device,
                    device_revision,
                    pet,
                    user,
                    activation,
                    profile,
                } => self
                    .characters
                    .authorize(context, self.world.body(context.actor).is_ok())
                    .map_err(|_| PetError::Unauthorized)
                    .and_then(|_| {
                        if self
                            .inventory
                            .item(device)
                            .is_none_or(|item| item.revision != device_revision)
                        {
                            return Err(PetError::MissingDevice);
                        }
                        if let Some(requirements) = &activation {
                            self.check_inventory_item_activation(context.actor, requirements)
                                .map_err(PetError::Activation)?;
                        }
                        self.register_pet_owner(context.actor, user)?;
                        if self.pets.owner_has_active(context.actor) {
                            profile.requirements.check(user).map_err(PetError::Use)?;
                            return self
                                .request_pet_stow(context.actor)
                                .map(|operation| PetDecision::Stowing { operation });
                        }
                        let operation =
                            self.propose_passive_pet(context.actor, device, pet, profile)?;
                        let ticket = self
                            .inventory
                            .pending_ticket(operation)
                            .cloned()
                            .ok_or(PetError::UnknownOperation)?;
                        let actor_revision = self
                            .characters
                            .get(context.actor)
                            .ok_or(PetError::MissingOwner)?
                            .revision();
                        self.inventory
                            .claim(operation)
                            .map_err(|_| PetError::Durability)?;
                        Ok(PetDecision::Proposed {
                            ticket,
                            actor_revision,
                            registry_revision: 0,
                            registry_after: vec![],
                        })
                    }),
                PetAction::Stow { owner } => self
                    .request_pet_stow(owner)
                    .map(|operation| PetDecision::Stowing { operation }),
                PetAction::Resolve { receipt, committed } => {
                    if self.pets.pending.contains_key(&receipt.operation)
                        || self.pets.pending_passive.contains_key(&receipt.operation)
                        || self.pets.retiring.contains_key(&receipt.operation)
                    {
                        let result = if committed {
                            self.confirm_inventory_committed(&receipt).map(|_| ())
                        } else {
                            self.reject_inventory(receipt.operation)
                        };
                        result
                            .map_err(|_| PetError::Durability)
                            .map(|()| PetDecision::Resolved {
                                operation: receipt.operation,
                                committed,
                            })
                    } else {
                        Err(PetError::UnknownOperation)
                    }
                }
            }
        };
        self.pet_outcomes.push_back(PetOutcome {
            correlation: command.correlation,
            result,
        });
    }
}
