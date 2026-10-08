//! Accepted NPC item changes use the player's existing event/property counters.
//! A bounded cursor retains large Give batches through reliable-channel pressure.
use super::*;
use bace_inventory::ItemPlace;
use bace_replication::{InventoryProjection as P, Sequences};
pub(super) struct Delivery {
    pub binding: bace_gameplay_api::CharacterBinding,
    pub ticket: bace_simulation::NpcInventoryTicket,
    pub gift: Option<Arc<crate::npc_items::PreparedNpcGift>>,
    pub cursor: usize,
}
impl GameRuntime {
    pub(super) fn poll_npc_inventory_output(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            if self.network_output.len() >= self.limits.messages {
                break;
            }
            let Some(delivery) = self.npc.inventory_output.front_mut() else {
                break;
            };
            let replica = self
                .players
                .replication(delivery.binding.actor)
                .ok_or("NPC inventory sequence owner absent")?;
            if replica.binding != delivery.binding {
                return Err("NPC inventory binding replaced".into());
            }
            // A disconnected recipient has no transport destination. The durable
            // item baseline remains the reconnect authority; no counter advances.
            if self
                .sessions
                .get(&replica.key)
                .is_some_and(|s| s.disconnected || s.terminated)
            {
                self.npc.inventory_output.pop_front();
                continue;
            }
            let mut order: Vec<_> = delivery
                .gift
                .as_ref()
                .map_or_else(Vec::new, |gift| gift.items.iter().map(|i| i.id).collect());
            order.extend(
                delivery
                    .ticket
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .filter(|c| {
                        c.before.as_ref().is_some_and(|before| {
                            before.stack != c.after.stack || c.after.place == ItemPlace::Removed
                        })
                    })
                    .map(|c| c.after.id),
            );
            let mut steps = Vec::new();
            let mut object = None;
            let mut fresh_sequence = None;
            let mut removal = None;
            if let Some(&id) = order.get(delivery.cursor) {
                let change = delivery
                    .ticket
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .find(|c| c.after.id == id)
                    .ok_or("NPC output change missing")?;
                if change.before.is_none() {
                    let gift = delivery
                        .gift
                        .as_ref()
                        .ok_or("NPC new item definition missing")?;
                    let mut description = gift
                        .descriptions
                        .get(&id)
                        .ok_or("NPC new item description missing")?
                        .clone();
                    let ItemPlace::Contained {
                        container,
                        equipped: 0,
                        ..
                    } = change.after.place
                    else {
                        return Err("NPC grant accepted placement".into());
                    };
                    description.game.options.container = Some(container.0);
                    object = Some(description);
                    if replica.item_properties.contains_key(&id)
                        || replica.item_properties.len() >= 1023
                    {
                        return Err("NPC fresh item sequence collision/capacity".into());
                    }
                    fresh_sequence = Some(id);
                    steps.push(P::Create(object.as_ref().expect("prepared accepted item")));
                    if change.after.is_container {
                        steps.push(P::Event(bace_wire::InventoryEvent::ViewContents {
                            container_id: id.0,
                            items: &[],
                        }));
                    }
                    let native = gift
                        .frozen
                        .iter()
                        .find(|f| f.entity.object_id == id.0)
                        .ok_or("NPC native item source missing")?;
                    let container_type = if native.entity.state.weenie_type == 21 {
                        1
                    } else if change.after.pack_slot {
                        2
                    } else {
                        0
                    };
                    steps.push(P::Event(bace_wire::InventoryEvent::PutInContainer {
                        object_id: id.0,
                        container_id: container.0,
                        placement: 0,
                        container_type,
                    }));
                } else if change.after.place == ItemPlace::Removed {
                    steps.push(P::Remove(id));
                    removal = Some(id);
                } else {
                    steps.push(P::Stack {
                        item: id,
                        quantity: change.after.stack,
                        value: change
                            .after
                            .stack
                            .checked_mul(change.after.unit_value)
                            .ok_or("NPC item value overflow")?,
                    });
                }
            } else {
                steps.push(P::PrivateProperty {
                    property: 5,
                    value: bace_wire::PropertyValue::Int(
                        i32::try_from(delivery.ticket.inventory.proposal.actor_burden)
                            .map_err(|_| "NPC actor burden packet bound")?,
                    ),
                });
            }
            let _keep_description_alive = &object;
            let objects = bace_wire::ObjectCodecLimits {
                max_message_bytes: self.limits.message_bytes,
                max_model_entries: 255,
                max_children: 255,
                max_restrictions: 1024,
                max_motion_commands: 4096,
                max_string_bytes: 4096,
            };
            let limits = bace_replication::BatchLimits {
                max_messages: self.limits.messages,
                max_bytes: self.limits.message_bytes,
                max_message_bytes: self.limits.message_bytes,
                max_string_bytes: 4096,
            };
            if let Some(id) = fresh_sequence {
                replica.item_properties.insert(
                    id,
                    Sequences::new(256).map_err(|e| format!("NPC item counter capacity: {e:?}"))?,
                );
            }
            let batch = match replica.events.project_inventory_with_actor(
                delivery.binding,
                &steps,
                &mut replica.item_properties,
                Some(&mut replica.properties),
                objects,
                limits,
            ) {
                Ok(batch) => batch,
                Err(error) => {
                    if let Some(id) = fresh_sequence {
                        replica.item_properties.remove(&id);
                    }
                    return Err(format!("NPC inventory packet: {error:?}"));
                }
            };
            if let Some(id) = removal {
                replica.item_properties.remove(&id);
            }
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch {
                    key: replica.key,
                    messages: batch
                        .messages
                        .into_iter()
                        .map(|m| (m.queue, m.bytes))
                        .collect(),
                });
            if delivery.cursor == order.len() {
                self.npc.inventory_output.pop_front();
            } else {
                delivery.cursor += 1;
            }
        }
        Ok(())
    }
}
