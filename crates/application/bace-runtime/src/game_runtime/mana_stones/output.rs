use super::*;
use bace_inventory::{ItemPlace, ManaStoneDecision};
use bace_replication::InventoryProjection as P;
use bace_wire::{PropertyValue, SimpleGameEvent};

impl GameRuntime {
    pub(super) fn project_mana_stone_output(&mut self) -> Result<(), String> {
        let Some(p) = self.mana_stones.pending.as_ref() else {
            return Ok(());
        };
        let key = p.key;
        if self
            .sessions
            .get(&key)
            .is_none_or(|session| session.disconnected || session.terminated)
        {
            if matches!(p.phase, Phase::Finished { .. } | Phase::Rejected(_)) {
                self.mana_stones.pending = None;
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
        let text: String;
        let mut steps = Vec::new();
        match &p.phase {
            Phase::Rejected(error) => {
                let code = match error {
                    ManaStoneError::NeedsMana => 0x04c8,
                    ManaStoneError::Ownership => 0x04be,
                    _ => 0x0036,
                };
                steps.push(P::Simple(SimpleGameEvent::UseDone(code)));
            }
            Phase::Finished {
                decision,
                committed,
            } => {
                if !committed {
                    steps.push(P::Simple(SimpleGameEvent::UseDone(0x0036)));
                } else {
                    match decision {
                        ManaStoneDecision::Empty => {}
                        ManaStoneDecision::AlreadyFull => {
                            text = format!("The {} is already full of mana.", p.target_name);
                            steps.push(P::System {
                                text: &text,
                                chat_type: 0,
                            });
                        }
                        ManaStoneDecision::NoEquippedItemsNeedMana => {
                            text = "You have no items equipped that need mana.".into();
                            steps.push(P::System {
                                text: &text,
                                chat_type: 0,
                            });
                        }
                        ManaStoneDecision::Extract {
                            target, drained, ..
                        } => {
                            let ticket = p
                                .ticket
                                .as_ref()
                                .ok_or("mana-stone extraction ticket missing")?;
                            let target_change = ticket
                                .inventory
                                .proposal
                                .changes
                                .iter()
                                .find(|change| change.after.id == *target)
                                .ok_or("mana-stone consumed target absent")?;
                            if matches!(target_change.after.place, ItemPlace::Removed) {
                                steps.push(P::Remove(*target));
                            } else {
                                steps.push(P::Stack {
                                    item: *target,
                                    quantity: target_change.after.stack,
                                    value: target_change
                                        .after
                                        .unit_value
                                        .saturating_mul(target_change.after.stack),
                                });
                            }
                            steps.push(P::Property {
                                item: p.stone,
                                property: 107,
                                value: PropertyValue::Int(*drained),
                            });
                            text = format!(
                                "The Mana Stone drains {} points of mana from the {}.\nThe {} is destroyed.",
                                comma(*drained),
                                p.target_name,
                                p.target_name
                            );
                            steps.push(P::System {
                                text: &text,
                                chat_type: 0,
                            });
                            steps.push(P::Property {
                                item: p.stone,
                                property: 18,
                                value: PropertyValue::Int(1),
                            });
                        }
                        ManaStoneDecision::Pour {
                            given,
                            targets,
                            destroy_stone,
                            ..
                        } => {
                            let target = targets
                                .first()
                                .ok_or("mana-stone target mana change missing")?;
                            steps.push(P::Property {
                                item: target.item,
                                property: 107,
                                value: PropertyValue::Int(
                                    target
                                        .after
                                        .current
                                        .ok_or("mana-stone target current absent")?,
                                ),
                            });
                            text = format!(
                                "The Mana Stone gives {} points of mana to the {}.",
                                comma(*given),
                                p.target_name
                            );
                            steps.push(P::System {
                                text: &text,
                                chat_type: 0,
                            });
                            if *destroy_stone {
                                let ticket = p.ticket.as_ref().ok_or("mana-stone destruction ticket missing")?;
                                let stone = ticket.inventory.proposal.changes.iter().find(|change| change.after.id == p.stone).ok_or("mana-stone changed source absent")?;
                                if matches!(stone.after.place, ItemPlace::Removed) { steps.push(P::Remove(p.stone)); }
                                else { steps.push(P::Stack { item: p.stone, quantity: stone.after.stack, value: stone.after.unit_value.saturating_mul(stone.after.stack) }); }
                                steps.push(P::System { text: "The Mana Stone is destroyed.", chat_type: 0 });
                            } else if p.ticket.as_ref().is_some_and(|ticket| matches!(&ticket.decision, ManaStoneDecision::Pour { stone, .. } if stone.after.current.is_none())) {
                                steps.push(P::Property { item: p.stone, property: 18, value: PropertyValue::Int(0) });
                            }
                        }
                    }
                    steps.push(P::Simple(SimpleGameEvent::UseDone(0)));
                }
            }
            _ => return Ok(()),
        }
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("mana-stone counters missing")?;
        if replica.key != key || replica.binding != binding {
            return Err("mana-stone output binding mismatch".into());
        }
        let max = self.limits.message_bytes;
        let batch = replica
            .events
            .project_inventory_with_actor(
                binding,
                &steps,
                &mut replica.item_properties,
                Some(&mut replica.properties),
                bace_wire::ObjectCodecLimits {
                    max_string_bytes: 4096,
                    max_model_entries: 4096,
                    max_children: 1024,
                    max_restrictions: 1024,
                    max_motion_commands: 1024,
                    max_message_bytes: max,
                },
                bace_replication::BatchLimits {
                    max_messages: 4096,
                    max_message_bytes: max,
                    max_bytes: 64 * 1024 * 1024,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|e| format!("mana-stone output projection: {e:?}"))?;
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|m| (m.queue, m.bytes))
                    .collect(),
            });
        self.mana_stones.pending = None;
        Ok(())
    }
}
fn comma(value: i32) -> String {
    let s = value.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
