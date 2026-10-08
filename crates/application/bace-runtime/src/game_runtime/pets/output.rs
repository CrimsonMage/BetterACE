//! Pet publication and device property output follow exact item receipts.
use super::*;
use bace_inventory::ActivationMessage;
use bace_replication::{BatchLimits, InventoryProjection};
use bace_wire::{ChatMessage, ObjectCodecLimits, PropertyValue, SimpleGameEvent};

impl GameRuntime {
    pub(super) fn project_pet_output(&mut self) -> Result<(), String> {
        let disconnected = self.pets.use_pending.as_ref().is_some_and(|pending| {
            self.sessions
                .get(&pending.key)
                .is_none_or(|session| session.disconnected)
        });
        if let Some(pending) = self.pets.use_pending.as_ref()
            && let use_action::Phase::Rejected(error) = pending.phase
        {
            if disconnected && !self.pets.service.requires_drain() {
                let actor = pending.context.actor;
                if let Some(completion) = self.pets.completions.front()
                    && completion.ticket.actor == actor
                    && !completion.committed
                {
                    let operation = completion.ticket.operation;
                    self.pets.completions.pop_front();
                    if matches!(self.pets.publications.front(), Some(PetEvent::Rejected { operation: id, owner }) if *id == operation && *owner == actor)
                    {
                        self.pets.publications.pop_front();
                    }
                }
                self.pets.use_pending = None;
                return Ok(());
            }
            return self.project_pet_rejection(error);
        }
        if let Some(pending) = self.pets.use_pending.as_ref()
            && let use_action::Phase::Blocked(error) = &pending.phase
        {
            return Err(format!("pet Use preparation retained: {error}"));
        }
        for _ in 0..self.limits.work_per_poll {
            let Some(event) = self.pets.publications.front() else {
                break;
            };
            match event {
                PetEvent::Spawned { pet, owner, device } => {
                    let Some(use_pending) = self.pets.use_pending.as_mut().filter(|p| {
                        p.pet == Some(*pet) && p.context.actor == *owner && p.device == *device
                    }) else {
                        break;
                    };
                    if !matches!(
                        use_pending.phase,
                        use_action::Phase::Saving | use_action::Phase::Published
                    ) {
                        break;
                    }
                    let prepared = use_pending
                        .prepared
                        .as_ref()
                        .ok_or("pet appearance owner missing")?;
                    let tick = u64::try_from(
                        self.last_elapsed.as_nanos().saturating_mul(30) / 1_000_000_000,
                    )
                    .map_err(|_| "pet visibility tick overflow")?;
                    prepared
                        .visibility
                        .register(&mut self.visibility.service, tick)
                        .map_err(|error| format!("pet visibility retained: {error:?}"))?;
                    self.pets.visible.insert(*pet);
                    use_pending.phase = use_action::Phase::Published;
                    let completed = use_pending.item_output;
                    self.pets.publications.pop_front();
                    if completed {
                        self.pets.use_pending = None;
                    }
                }
                PetEvent::Despawned { pet, owner } => {
                    if self.pets.visible.contains(pet) {
                        let tick = u64::try_from(
                            self.last_elapsed.as_nanos().saturating_mul(30) / 1_000_000_000,
                        )
                        .map_err(|_| "pet retirement tick overflow")?;
                        self.visibility
                            .service
                            .retire_object(*pet, tick)
                            .map_err(|error| {
                                format!("pet visibility retirement retained: {error:?}")
                            })?;
                        self.pets.visible.remove(pet);
                    }
                    if let Some(pending) = self.pets.use_pending.as_mut()
                        && *owner == pending.context.actor
                        && let use_action::Phase::WaitingStow(operation) = pending.phase
                    {
                        pending.phase = use_action::Phase::StowRetired(operation);
                    }
                    self.pets.publications.pop_front();
                    if self.pets.use_pending.as_ref().is_some_and(|pending| {
                        pending.item_output
                            && matches!(pending.phase, use_action::Phase::StowRetired(_))
                    }) {
                        self.pets.use_pending = None;
                    }
                }
                PetEvent::Rejected { operation, owner } => {
                    if let Some(pending) = self
                        .pets
                        .use_pending
                        .as_mut()
                        .filter(|p| p.context.actor == *owner)
                        && self
                            .pets
                            .completions
                            .iter()
                            .any(|c| c.ticket.operation == *operation && !c.committed)
                    {
                        pending.phase =
                            use_action::Phase::Rejected(bace_simulation::PetError::Durability);
                    }
                    break;
                }
                _ => break,
            }
        }
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let Some(completion) = self.pets.completions.front() else {
            return Ok(());
        };
        if !completion.committed {
            if disconnected && completion.summon {
                let rejected = self.pets.use_pending.as_ref().is_some_and(|pending| {
                    matches!(pending.phase, use_action::Phase::Saving)
                        && pending.context.actor == completion.ticket.actor
                        && completion.ticket.proposal.changes.len() == 1
                        && completion.ticket.proposal.changes[0].after.id == pending.device
                });
                if !rejected {
                    return Err("disconnected pet rejection owner mismatch".into());
                }
                self.pets.completions.pop_front();
                self.pets.use_pending = None;
                return Ok(());
            }
            if disconnected
                && let Some(pending) = self.pets.use_pending.as_ref()
                && matches!(pending.phase, use_action::Phase::WaitingStow(operation) | use_action::Phase::StowRetired(operation) if operation == completion.ticket.operation)
            {
                let operation = completion.ticket.operation;
                let actor = completion.ticket.actor;
                self.pets.completions.pop_front();
                if matches!(self.pets.publications.front(), Some(PetEvent::Rejected { operation: id, owner }) if *id == operation && *owner == actor)
                {
                    self.pets.publications.pop_front();
                }
                self.pets.use_pending = None;
                return Ok(());
            }
            return Err("pet valuable operation rejected; owner output retained".into());
        }
        if !completion.summon {
            if let Some(pending) = self.pets.use_pending.as_ref()
                && matches!(pending.phase, use_action::Phase::WaitingStow(operation) | use_action::Phase::StowRetired(operation) if operation == completion.ticket.operation)
            {
                if completion.ticket.actor != pending.context.actor {
                    return Err("passive pet stow owner mismatch".into());
                }
                if !disconnected {
                    let binding = bace_gameplay_api::CharacterBinding {
                        actor: pending.context.actor,
                        account: pending.context.account,
                        session: pending.context.session,
                    };
                    let replica = self
                        .players
                        .replication(binding.actor)
                        .ok_or("passive pet stow replica missing")?;
                    if replica.key != pending.key || replica.binding != binding {
                        return Err("passive pet stow binding mismatch".into());
                    }
                    let max = self.limits.message_bytes;
                    let batch = replica
                        .events
                        .project_inventory(
                            binding,
                            &[InventoryProjection::Simple(SimpleGameEvent::UseDone(0))],
                            &mut replica.item_properties,
                            ObjectCodecLimits {
                                max_string_bytes: 4096,
                                max_model_entries: 4096,
                                max_children: 1024,
                                max_restrictions: 1024,
                                max_motion_commands: 1024,
                                max_message_bytes: max,
                            },
                            BatchLimits {
                                max_messages: 4096,
                                max_message_bytes: max,
                                max_bytes: 64 * 1024 * 1024,
                                max_string_bytes: 4096,
                            },
                        )
                        .map_err(|error| format!("passive pet stow output retained: {error:?}"))?;
                    self.network_output
                        .push_back(NetworkCommand::SendOrderedBatch {
                            key: pending.key,
                            messages: batch
                                .messages
                                .into_iter()
                                .map(|message| (message.queue, message.bytes))
                                .collect(),
                        });
                }
                let pending = self.pets.use_pending.as_mut().expect("stow owner");
                pending.item_output = true;
                if matches!(pending.phase, use_action::Phase::StowRetired(_)) {
                    self.pets.use_pending = None;
                }
            }
            self.pets.completions.pop_front();
            return Ok(());
        }
        let pending = self
            .pets
            .use_pending
            .as_ref()
            .ok_or("pet Use completion owner missing")?;
        if !matches!(
            pending.phase,
            use_action::Phase::Saving | use_action::Phase::Published
        ) || completion.ticket.actor != pending.context.actor
            || completion.ticket.proposal.changes.len() != 1
        {
            return Err("pet Use completion identity mismatch".into());
        }
        let change = &completion.ticket.proposal.changes[0];
        if change.after.id != pending.device || !change.after.active_pet {
            return Err("pet committed device state mismatch".into());
        }
        if disconnected {
            self.pets.completions.pop_front();
            let pending = self.pets.use_pending.as_mut().expect("committed pet owner");
            pending.item_output = true;
            if matches!(pending.phase, use_action::Phase::Published) {
                self.pets.use_pending = None;
            }
            return Ok(());
        }
        let binding = bace_gameplay_api::CharacterBinding {
            actor: pending.context.actor,
            account: pending.context.account,
            session: pending.context.session,
        };
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("pet committed replication owner missing")?;
        if replica.key != pending.key || replica.binding != binding {
            return Err("pet committed session binding mismatch".into());
        }
        let mut steps = Vec::with_capacity(2);
        if change.before.as_ref().and_then(|before| before.structure) != change.after.structure {
            let structure = change.after.structure.ok_or("pet structure disappeared")?;
            steps.push(InventoryProjection::Property {
                item: pending.device,
                property: 92,
                value: PropertyValue::Int(
                    i32::try_from(structure).map_err(|_| "pet structure overflow")?,
                ),
            });
        }
        steps.push(InventoryProjection::Simple(SimpleGameEvent::UseDone(0)));
        let max = self.limits.message_bytes;
        let batch = replica
            .events
            .project_inventory(
                binding,
                &steps,
                &mut replica.item_properties,
                ObjectCodecLimits {
                    max_string_bytes: 4096,
                    max_model_entries: 4096,
                    max_children: 1024,
                    max_restrictions: 1024,
                    max_motion_commands: 1024,
                    max_message_bytes: max,
                },
                BatchLimits {
                    max_messages: 4096,
                    max_message_bytes: max,
                    max_bytes: 64 * 1024 * 1024,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|error| format!("pet device output retained: {error:?}"))?;
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key: pending.key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|message| (message.queue, message.bytes))
                    .collect(),
            });
        self.pets.completions.pop_front();
        let pending = self.pets.use_pending.as_mut().expect("projected pet owner");
        pending.item_output = true;
        if matches!(pending.phase, use_action::Phase::Published) {
            self.pets.use_pending = None;
        }
        Ok(())
    }

    fn project_pet_rejection(&mut self, error: bace_simulation::PetError) -> Result<(), String> {
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let pending = self
            .pets
            .use_pending
            .as_ref()
            .ok_or("pet rejection owner missing")?;
        let notice = match error {
            bace_simulation::PetError::Activation(failure) => Some(
                failure
                    .message(&pending.device_name)
                    .ok_or("pet activation lacks source rejection output")?,
            ),
            _ => None,
        };
        let chat = match error {
            bace_simulation::PetError::Use(bace_simulation::PetUseError::NoCharges) => {
                Some("Your summoning device does not have enough charges to function!")
            }
            _ => None,
        };
        if notice.is_none() && chat.is_none() {
            return Err(format!(
                "pet Use rejection source output retained: {error:?}"
            ));
        }
        let binding = bace_gameplay_api::CharacterBinding {
            actor: pending.context.actor,
            account: pending.context.account,
            session: pending.context.session,
        };
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("pet rejection replication owner missing")?;
        if replica.key != pending.key || replica.binding != binding {
            return Err("pet rejection session binding mismatch".into());
        }
        let mut steps = Vec::with_capacity(2);
        match notice.as_ref() {
            Some(ActivationMessage::Error(code)) => steps.push(InventoryProjection::Simple(
                SimpleGameEvent::WeenieError(*code),
            )),
            Some(ActivationMessage::ErrorWithString { error, text }) => {
                steps.push(InventoryProjection::Group(
                    bace_wire::GroupEvent::ErrorWithString { code: *error, text },
                ))
            }
            Some(ActivationMessage::Transient(text)) => steps.push(InventoryProjection::Social(
                bace_wire::SocialEvent::Transient(text),
            )),
            None => {}
        }
        steps.push(InventoryProjection::Simple(SimpleGameEvent::UseDone(0)));
        let max = self.limits.message_bytes;
        let batch = replica
            .events
            .project_inventory(
                binding,
                &steps,
                &mut replica.item_properties,
                ObjectCodecLimits {
                    max_string_bytes: 4096,
                    max_model_entries: 4096,
                    max_children: 1024,
                    max_restrictions: 1024,
                    max_motion_commands: 1024,
                    max_message_bytes: max,
                },
                BatchLimits {
                    max_messages: 4096,
                    max_message_bytes: max,
                    max_bytes: 64 * 1024 * 1024,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|error| format!("pet rejection output retained: {error:?}"))?;
        let mut messages: Vec<_> = batch
            .messages
            .into_iter()
            .map(|message| (message.queue, message.bytes))
            .collect();
        if let Some(text) = chat {
            messages.insert(
                0,
                (
                    9,
                    ChatMessage::System { text, chat_type: 0 }
                        .encode()
                        .map_err(|error| format!("pet source chat retained: {error:?}"))?,
                ),
            );
        }
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key: pending.key,
                messages,
            });
        self.pets.use_pending = None;
        Ok(())
    }
}
