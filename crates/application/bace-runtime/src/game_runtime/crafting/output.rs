//! Committed item updates use the same canonical sequence owners as inventory.
use super::*;
use bace_replication::{BatchLimits, InventoryProjection};
use bace_wire::{CraftingEvent, ObjectCodecLimits, SimpleGameEvent};
impl GameRuntime {
    pub(super) fn project_crafting_output(&mut self) -> Result<(), String> {
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let Some(p) = self.crafting.pending.as_ref() else {
            return Ok(());
        };
        let key = p.key;
        let binding = p.binding;
        if let Some((style, command)) = p.motion {
            if !self.observer_room(1, 4096) {
                return Ok(());
            }
            let lum = p
                .input
                .as_ref()
                .ok_or("crafting animation input missing")?
                .context
                .chance
                .lum_craft;
            let text = (lum > 0).then(|| {
                format!("Your Aura of the Craftman augmentation increased your skill by {lum}!")
            });
            let mut messages = Vec::new();
            if let Some(text) = &text {
                messages.push((
                    9,
                    bace_wire::ChatMessage::System {
                        text,
                        chat_type: 0x00,
                    }
                    .encode()
                    .map_err(|e| e.to_string())?,
                ));
            }
            let replica = self
                .players
                .replication(binding.actor)
                .ok_or("crafting motion counters missing")?;
            let view = bace_wire::MovementDescription {
                autonomous: false,
                motion_flags: 0,
                current_style: style as u16,
                body: bace_wire::MotionBody::State {
                    state: bace_wire::InterpretedMotion {
                        current_style: Some(style as u16),
                        forward_command: Some(3),
                        commands: vec![bace_wire::MotionCommandItem {
                            raw_command: command as u16,
                            sequence: 0,
                            autonomous: false,
                            speed: 1.0,
                        }],
                        ..Default::default()
                    },
                    sticky_object: None,
                },
            };
            let motion = bace_replication::project_server_motion(
                binding.actor.0,
                &view,
                &mut replica.properties,
                limits(self.limits.message_bytes),
            )
            .map_err(|e| format!("crafting motion projection: {e:?}"))?;
            messages.push((motion.queue, motion.bytes.clone()));
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch { key, messages });
            self.retain_observer_messages(vec![(binding.actor, vec![motion])])?;
            self.crafting
                .pending
                .as_mut()
                .expect("retained motion")
                .motion = None;
            return Ok(());
        }
        if let Phase::Quoted(chance) = p.phase {
            let token = u32::try_from(self.token()?)
                .map_err(|_| "crafting confirmation token exhausted")?;
            let p = self.crafting.pending.as_ref().expect("quote owner");
            let input = p.input.clone().ok_or("quoted input missing")?;
            let native = p.native.clone().ok_or("quoted source recipe missing")?;
            let assets = p.assets.clone().ok_or("crafting cold assets missing")?;
            let text = bace_crafting::tinker_confirmation_text(
                chance.probability,
                input.context.chance.imbue && input.context.chance.imbue_augmentation,
            )
            .map_err(|e| format!("crafting confirmation: {e:?}"))?;
            let replica = self
                .players
                .replication(binding.actor)
                .ok_or("crafting canonical counters missing")?;
            let batch = replica
                .events
                .project_inventory(
                    binding,
                    &[
                        InventoryProjection::Crafting(CraftingEvent::ConfirmationRequest {
                            confirmation_type: 5,
                            context: token,
                            text: &text,
                        }),
                        InventoryProjection::Simple(SimpleGameEvent::UseDone(0)),
                    ],
                    &mut replica.item_properties,
                    objects(self.limits.message_bytes),
                    limits(self.limits.message_bytes),
                )
                .map_err(|e| format!("crafting confirmation projection: {e:?}"))?;
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch {
                    key,
                    messages: batch
                        .messages
                        .into_iter()
                        .map(|m| (m.queue, m.bytes))
                        .collect(),
                });
            self.crafting.quotes.insert(
                key,
                Quote {
                    token,
                    expires_at: self.last_elapsed.saturating_add(Duration::from_secs(60)),
                    input,
                    native,
                    assets,
                },
            );
            self.crafting.pending = None;
            return Ok(());
        }
        if let Phase::Failed(error) = &p.phase {
            let error = error.clone();
            let message = if p.rejection == Some(bace_crafting::CraftError::Requirement) {
                match (&p.native, &p.input) {
                    (Some(native), Some(input)) => native
                        .requirement_failure_message(&input.context, &input.source, &input.target)
                        .map_err(|e| format!("requirement message: {e:?}"))?,
                    _ => None,
                }
            } else {
                None
            };
            let code = if p.rejection == Some(bace_crafting::CraftError::Busy) {
                0x001d
            } else {
                0x0437
            };
            let mut steps = Vec::new();
            if let Some(text) = message {
                steps.push(InventoryProjection::System {
                    text,
                    chat_type: 0x18,
                });
            }
            steps.push(InventoryProjection::Simple(SimpleGameEvent::UseDone(code)));
            let replica = self
                .players
                .replication(binding.actor)
                .ok_or("crafting rejection counters missing")?;
            let batch = replica
                .events
                .project_inventory(
                    binding,
                    &steps,
                    &mut replica.item_properties,
                    objects(self.limits.message_bytes),
                    limits(self.limits.message_bytes),
                )
                .map_err(|e| format!("crafting rejection projection: {e:?}"))?;
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch {
                    key,
                    messages: batch
                        .messages
                        .into_iter()
                        .map(|m| (m.queue, m.bytes))
                        .collect(),
                });
            self.crafting.failures.insert(key, error);
            self.crafting.quotes.remove(&key);
            self.crafting.pending = None;
            return Ok(());
        }
        if matches!(p.phase, Phase::Done) {
            let replica = self
                .players
                .replication(binding.actor)
                .ok_or("crafting cancellation counters missing")?;
            let batch = replica
                .events
                .project_inventory(
                    binding,
                    &[InventoryProjection::Simple(SimpleGameEvent::WeenieError(
                        0x0526,
                    ))],
                    &mut replica.item_properties,
                    objects(self.limits.message_bytes),
                    limits(self.limits.message_bytes),
                )
                .map_err(|e| format!("crafting cancellation: {e:?}"))?;
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch {
                    key,
                    messages: batch
                        .messages
                        .into_iter()
                        .map(|m| (m.queue, m.bytes))
                        .collect(),
                });
            self.crafting.quotes.remove(&key);
            self.crafting.pending = None;
            return Ok(());
        }
        let Some(completion) = &p.completion else {
            return Ok(());
        };
        if !self.observer_room(6, 64 * 1024) {
            return Ok(());
        }
        if !completion.committed {
            self.crafting.pending.as_mut().expect("completion").phase =
                Phase::Failed("crafting persistence rejected".into());
            return Ok(());
        }
        let bace_simulation::CraftingDecision::Tinker(decision) = &completion.work.ticket.decision
        else {
            return Err("non-tinker completion on live tinker lane".into());
        };
        let loading = self
            .sessions
            .get(&key)
            .and_then(|s| s.loading.as_ref())
            .ok_or("crafting output session missing")?;
        let appearance = &p
            .assets
            .as_ref()
            .ok_or("crafting appearance assets missing")?
            .appearance;
        let character = loading
            .character_assets
            .as_ref()
            .ok_or("crafting CharGen missing")?;
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("crafting output counters missing")?;
        let input = p
            .input
            .as_ref()
            .ok_or("crafting completion input missing")?;
        let native = p
            .native
            .as_ref()
            .ok_or("crafting completion source recipe missing")?;
        let branch = if decision.success {
            &native.recipe.success
        } else {
            &native.recipe.failure
        };
        let modified = |id: u32| {
            let participant = if id == input.source.id {
                bace_crafting::Participant::Source
            } else {
                bace_crafting::Participant::Target
            };
            branch
                .mutations
                .iter()
                .any(|m| m.participant == participant)
                || participant == bace_crafting::Participant::Target
                    && native.recipe.increment_tinker_count
        };
        let mut updated = BTreeMap::new();
        for id in [input.source.id, input.target.id] {
            if !modified(id) {
                continue;
            }
            let Some(change) = completion
                .work
                .ticket
                .inventory
                .changes
                .iter()
                .find(|c| c.after.id.0 == id)
            else {
                continue;
            };
            if change.after.place == bace_inventory::ItemPlace::Removed {
                continue;
            }
            let saved = self
                .online_saves
                .inventory_baseline(binding.actor.0, id)
                .ok_or("committed crafting item baseline missing")?;
            let seq = replica
                .item_properties
                .get(&change.after.id)
                .ok_or("crafting item counter owner missing")?;
            let object = crate::player_entry::prepare_entry_object(
                id,
                &saved.entity.state,
                crate::player_entry::prepare_item_model(
                    &saved.entity.state,
                    &appearance.borrowed(character.char_gen()),
                )?,
                super::super::inventory::output::committed_state(saved, seq)?,
            )?;
            updated.insert(change.after.id, object);
        }
        let mut steps = Vec::new();
        // Source consumes target first, then tool; UpdateObj follows source then target.
        for id in [input.target.id, input.source.id] {
            if let Some(change) = completion
                .work
                .ticket
                .inventory
                .changes
                .iter()
                .find(|c| c.after.id.0 == id)
            {
                if change.after.place == bace_inventory::ItemPlace::Removed {
                    steps.push(InventoryProjection::Remove(change.after.id));
                } else if change
                    .before
                    .as_ref()
                    .is_some_and(|b| b.stack != change.after.stack)
                {
                    steps.push(InventoryProjection::Stack {
                        item: change.after.id,
                        quantity: change.after.stack,
                        value: change
                            .after
                            .unit_value
                            .checked_mul(change.after.stack)
                            .ok_or("crafting stack value overflow")?,
                    });
                }
            }
            let triggered = if id == input.target.id {
                decision.destroy_target
            } else {
                decision.destroy_source
            };
            let message = match (decision.success, id == input.target.id) {
                (true, true) => native.source.success_destroy_target_message.as_deref(),
                (true, false) => native.source.success_destroy_source_message.as_deref(),
                (false, true) => native.source.fail_destroy_target_message.as_deref(),
                (false, false) => native.source.fail_destroy_source_message.as_deref(),
            };
            if triggered && let Some(text) = message.filter(|m| !m.is_empty()) {
                steps.push(InventoryProjection::System {
                    text,
                    chat_type: 0x18,
                });
            }
        }
        for update in &decision.property_updates {
            let item = bace_types::EntityId(match update.participant {
                bace_crafting::Participant::Source => input.source.id,
                bace_crafting::Participant::Target => input.target.id,
                bace_crafting::Participant::Actor => binding.actor.0,
            });
            let value = match &update.value {
                bace_crafting::PropertyValue::Bool(v) => bace_wire::PropertyValue::Bool(*v),
                bace_crafting::PropertyValue::Int(v) => bace_wire::PropertyValue::Int(*v),
                bace_crafting::PropertyValue::Int64(v) => bace_wire::PropertyValue::Int64(*v),
                bace_crafting::PropertyValue::Float(v) => bace_wire::PropertyValue::Float(*v),
                bace_crafting::PropertyValue::String(v) => bace_wire::PropertyValue::String(v),
                bace_crafting::PropertyValue::DataId(v) => bace_wire::PropertyValue::DataId(*v),
                bace_crafting::PropertyValue::InstanceId(v) => {
                    bace_wire::PropertyValue::InstanceId(*v)
                }
                bace_crafting::PropertyValue::SpellBook(_) => {
                    return Err("spellbook is not a scalar property packet".into());
                }
            };
            steps.push(InventoryProjection::Property {
                item,
                property: update.key.id,
                value,
            });
        }
        let text = bace_crafting::tinker_result_text(
            &input.context,
            &input.source,
            &input.target,
            &p.assets.as_ref().expect("validated assets").materials,
            decision.success,
        )
        .map_err(|e| format!("tinkering result text: {e:?}"))?;
        steps.push(InventoryProjection::System {
            text: &text,
            chat_type: 0x18,
        });
        for id in [input.source.id, input.target.id] {
            if let Some(object) = updated.get(&bace_types::EntityId(id)) {
                steps.push(InventoryProjection::Update(object));
            }
        }
        let mut observer_messages = vec![(
            binding.actor,
            9,
            bace_wire::ChatMessage::System {
                text: &text,
                chat_type: 0x18,
            }
            .encode()
            .map_err(|e| e.to_string())?,
        )];
        for id in [input.source.id, input.target.id] {
            if let Some(object) = updated.get(&bace_types::EntityId(id)) {
                observer_messages.push((
                    bace_types::EntityId(id),
                    10,
                    object
                        .encode_update(objects(self.limits.message_bytes))
                        .map_err(|e| e.to_string())?,
                ));
            }
        }
        let proficiency = completion.work.ticket.proficiency.as_ref().map(|p| {
            bace_replication::CraftingProficiencyProjection {
                skill: (p.change.spent > 0).then(|| bace_replication::CraftingSkillSpend {
                    after: p.change.after,
                    available: p.change.earned.experience.before_available
                        - u64::from(p.change.spent),
                    rank_changed: p.change.before.ranks != p.change.after.ranks,
                    base: p.skill_base,
                    maximum: p.skill_maximum,
                }),
                experience: p.experience.as_ref(),
            }
        });
        let batch = bace_replication::project_crafting_commit(
            &mut replica.events,
            binding,
            &steps,
            &mut replica.item_properties,
            &mut replica.properties,
            proficiency,
            !p.confirm,
            objects(self.limits.message_bytes),
            limits(self.limits.message_bytes),
        )
        .map_err(|e| format!("committed crafting projection: {e:?}"))?;
        observer_messages.extend(
            batch
                .observers
                .into_iter()
                .map(|m| (binding.actor, m.queue, m.bytes)),
        );
        let batch = batch.owner;
        for change in &completion.work.ticket.inventory.changes {
            if change.after.place == bace_inventory::ItemPlace::Removed {
                replica.item_properties.remove(&change.after.id);
            }
        }
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|m| (m.queue, m.bytes))
                    .collect(),
            });
        self.retain_observer_messages(
            observer_messages
                .into_iter()
                .map(|(actor, queue, bytes)| {
                    (
                        actor,
                        vec![bace_replication::ReplicationMessage { queue, bytes }],
                    )
                })
                .collect(),
        )?;
        self.crafting.quotes.remove(&key);
        self.crafting.pending = None;
        Ok(())
    }
}
fn objects(max: usize) -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_message_bytes: max,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 1024,
    }
}
fn limits(max: usize) -> BatchLimits {
    BatchLimits {
        max_messages: 4096,
        max_bytes: 64 * 1024 * 1024,
        max_message_bytes: max,
        max_string_bytes: 4096,
    }
}
