use super::*;
use bace_inventory::ItemPlace;
use bace_replication::InventoryProjection as P;
use bace_wire::{PropertyValue, SimpleGameEvent};

impl GameRuntime {
    pub(super) fn project_healing_kit_output(&mut self) -> Result<(), String> {
        let Some(p) = self.healing_kits.pending.as_ref() else {
            return Ok(());
        };
        let key = p.key;
        if self
            .sessions
            .get(&key)
            .is_none_or(|session| session.disconnected || session.terminated)
        {
            if matches!(p.phase, Phase::Rejected(_) | Phase::Finished) {
                self.healing_kits.pending = None;
            }
            return Ok(());
        }
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let binding = CharacterBinding {
            actor: p.context.actor,
            account: p.context.account,
            session: p.context.session,
        };
        if let Some((style, command)) = p.motion {
            if !self.observer_room(1, 4096) {
                return Ok(());
            }
            let replica = self
                .players
                .replication(binding.actor)
                .ok_or("healing motion counters missing")?;
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
            .map_err(|error| format!("healing motion projection: {error:?}"))?;
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch {
                    key,
                    messages: vec![(motion.queue, motion.bytes.clone())],
                });
            self.retain_observer_messages(vec![(binding.actor, vec![motion])])?;
            self.healing_kits
                .pending
                .as_mut()
                .expect("retained healing motion")
                .motion = None;
            return Ok(());
        }
        let mut text = String::new();
        let mut steps = Vec::new();
        match p.phase {
            Phase::Rejected(error) => {
                let code = match error {
                    bace_simulation::HealingKitError::NotTrained => 0x04fc,
                    bace_simulation::HealingKitError::FullVital => 0,
                    bace_simulation::HealingKitError::Busy => 0x001d,
                    bace_simulation::HealingKitError::UnsupportedTarget => 0x04fe,
                    _ => 0x0036,
                };
                if matches!(error, bace_simulation::HealingKitError::FullVital) {
                    match p.vital {
                        Some(bace_entity::EntityVital::Health) => {
                            steps.push(P::Group(bace_wire::GroupEvent::ErrorWithString {
                                code: 0x04ff,
                                text: &p.target_name,
                            }))
                        }
                        Some(bace_entity::EntityVital::Stamina) => {
                            text = format!("{} is already at full stamina!", p.target_name);
                            steps.push(P::Social(bace_wire::SocialEvent::Transient(&text)));
                        }
                        Some(bace_entity::EntityVital::Mana) => {
                            text = format!("{} is already at full mana!", p.target_name);
                            steps.push(P::Social(bace_wire::SocialEvent::Transient(&text)));
                        }
                        None => return Err("healing full-vital result without source vital".into()),
                    }
                }
                steps.push(P::Simple(SimpleGameEvent::UseDone(code)));
            }
            Phase::Finished => {
                let completion = p.completion.as_ref().ok_or("healing completion missing")?;
                if !completion.committed {
                    steps.push(P::Simple(SimpleGameEvent::UseDone(0x0036)));
                } else {
                    let ticket = &completion.ticket;
                    if let Some(inventory) = &ticket.inventory {
                        let item = inventory
                            .proposal
                            .changes
                            .iter()
                            .find(|c| c.after.id == p.item)
                            .ok_or("healing kit charge missing")?;
                        let remaining = item
                            .after
                            .structure
                            .ok_or("healing kit charge Structure missing")?;
                        steps.push(P::Property {
                            item: p.item,
                            property: 92,
                            value: PropertyValue::Int(
                                i32::try_from(remaining)
                                    .map_err(|_| "healing Structure overflow")?,
                            ),
                        });
                        if !matches!(item.after.place, ItemPlace::Removed) {
                            steps.push(P::Property {
                                item: p.item,
                                property: 19,
                                value: PropertyValue::Int(
                                    i32::try_from(item.after.unit_value)
                                        .map_err(|_| "healing Value overflow")?,
                                ),
                            });
                        }
                        text = if remaining == 0 {
                            format!(" Your {} is used up.", p.name)
                        } else {
                            format!(
                                " Your {} has {remaining} use{} left.",
                                p.name,
                                if remaining == 1 { "" } else { "s" }
                            )
                        };
                        if matches!(item.after.place, ItemPlace::Removed) {
                            steps.push(P::Remove(p.item));
                        }
                    }
                    if ticket.success {
                        for mutation in &ticket.vitals {
                            let vital = match mutation.vital {
                                bace_entity::EntityVital::Health => 2,
                                bace_entity::EntityVital::Stamina => 4,
                                bace_entity::EntityVital::Mana => 6,
                            };
                            steps.push(P::Vital {
                                vital,
                                current: mutation.after,
                            });
                        }
                        let target_vital = ticket
                            .vitals
                            .iter()
                            .find(|m| m.after > m.before)
                            .map(|m| match m.vital {
                                bace_entity::EntityVital::Health => "MaxHealth",
                                bace_entity::EntityVital::Stamina => "MaxStamina",
                                bace_entity::EntityVital::Mana => "MaxMana",
                            })
                            .unwrap_or("MaxHealth");
                        text = format!(
                            "You {}heal yourself for {} {} points.{}",
                            if ticket.critical { "expertly " } else { "" },
                            ticket.heal_amount,
                            target_vital,
                            text
                        );
                    } else {
                        text = format!("You fail to heal yourself.{text}");
                    }
                    steps.push(P::System {
                        text: &text,
                        chat_type: 0,
                    });
                    steps.push(P::Simple(SimpleGameEvent::UseDone(0)));
                }
            }
            _ => return Ok(()),
        }
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("healing canonical counters missing")?;
        if replica.key != key || replica.binding != binding {
            return Err("healing output binding mismatch".into());
        }
        let max = self.limits.message_bytes;
        let batch = replica
            .events
            .project_inventory_with_actor(
                binding,
                &steps,
                &mut replica.item_properties,
                Some(&mut replica.properties),
                objects(max),
                limits(max),
            )
            .map_err(|e| format!("healing output projection: {e:?}"))?;
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|m| (m.queue, m.bytes))
                    .collect(),
            });
        self.healing_kits.pending = None;
        Ok(())
    }
}

fn objects(max: usize) -> bace_wire::ObjectCodecLimits {
    bace_wire::ObjectCodecLimits {
        max_string_bytes: 4096,
        max_model_entries: 4096,
        max_children: 1024,
        max_restrictions: 1024,
        max_motion_commands: 1024,
        max_message_bytes: max,
    }
}
fn limits(max: usize) -> bace_replication::BatchLimits {
    bace_replication::BatchLimits {
        max_messages: 4096,
        max_message_bytes: max,
        max_bytes: 64 * 1024 * 1024,
        max_string_bytes: 4096,
    }
}
