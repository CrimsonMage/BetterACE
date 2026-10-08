//! Durable equipment publication retains visibility and reliable obligations.
use super::*;
use crate::inventory_equipment_output::EquipmentVisibilityUpdate;
use bace_inventory::{ActivationMessage, ItemPlace};
use bace_replication::{InventoryProjection as P, SequenceKind};
use bace_wire::{InventoryEvent, MagicEvent, PropertyValue};
impl GameRuntime {
    pub fn pending_inventory_equipment_visibility(&self) -> Option<&EquipmentVisibilityUpdate> {
        self.inventory
            .equipment
            .as_ref()
            .filter(|g| !g.visibility_accepted)?
            .visibility
            .as_ref()
    }
    /// Only after the visibility owner accepts this exact durable model/child graph.
    pub fn acknowledge_inventory_equipment_visibility(
        &mut self,
        operation: u64,
    ) -> Result<(), String> {
        let gear = self
            .inventory
            .equipment
            .as_mut()
            .ok_or("equipment visibility owner missing")?;
        if gear
            .visibility
            .as_ref()
            .is_none_or(|v| v.operation != operation)
            || gear.visibility_accepted
        {
            return Err("equipment visibility correlation".into());
        }
        gear.visibility_accepted = true;
        Ok(())
    }
    pub(super) fn project_inventory_equipment(&mut self) -> Result<(), String> {
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        if self
            .inventory
            .equipment
            .as_ref()
            .is_none_or(|g| g.visibility.is_none())
        {
            let fresh = self
                .inventory
                .pending
                .as_ref()
                .and_then(|p| p.completion.as_ref())
                .and_then(|c| c.work.fresh.as_ref())
                .map(|stack| stack.item.id);
            if let Some(id) = fresh
                && !self
                    .inventory
                    .equipment
                    .as_ref()
                    .is_some_and(|g| g.fresh_registered)
            {
                let actor = self
                    .inventory
                    .pending
                    .as_ref()
                    .expect("equipment owner")
                    .binding
                    .actor;
                let replica = self
                    .players
                    .replication(actor)
                    .ok_or("fresh equipment replica missing")?;
                if replica.item_properties.len() >= 1023
                    || replica.item_properties.contains_key(&id)
                {
                    return Err("fresh equipment sequence capacity or duplicate".into());
                }
                replica.item_properties.insert(
                    id,
                    bace_replication::Sequences::new(256)
                        .map_err(|e| format!("fresh equipment sequences: {e:?}"))?,
                );
                self.inventory
                    .equipment
                    .as_mut()
                    .expect("equipment owner")
                    .fresh_registered = true;
            }
            let value = self.prepare_equipment_visibility()?;
            self.inventory
                .equipment
                .as_mut()
                .ok_or("equipment output owner missing")?
                .visibility = Some(value);
        }
        if !self
            .inventory
            .equipment
            .as_ref()
            .is_some_and(|g| g.visibility_accepted)
        {
            let update = self
                .inventory
                .equipment
                .as_ref()
                .and_then(|g| g.visibility.as_ref())
                .ok_or("equipment visibility handoff missing")?;
            let operation = update.operation;
            match self.visibility.service.replace_equipment_blueprint(update) {
                Ok(()) => self.acknowledge_inventory_equipment_visibility(operation)?,
                Err(crate::visibility_service::VisibilityServiceError::Capacity) => return Ok(()),
                Err(error) => return Err(format!("equipment visibility retained: {error:?}")),
            }
        }
        // Reserve conservative bounded room before advancing canonical counters.
        // The actual retained observer payload is checked again below.
        if !self.observer_room(1, 1024 * 1024) {
            return Ok(());
        }
        let pending = self
            .inventory
            .pending
            .as_ref()
            .ok_or("equipment pending owner missing")?;
        let completion = pending
            .completion
            .as_ref()
            .ok_or("equipment completion missing")?;
        let operation = &completion.work.operation;
        let patch = operation
            .equipment
            .as_ref()
            .ok_or("equipment patch missing")?;
        let visibility = self
            .inventory
            .equipment
            .as_ref()
            .and_then(|g| g.visibility.as_ref())
            .ok_or("equipment visibility missing")?;
        let actor = operation.binding.actor;
        let key = pending.key;
        let change = operation
            .ticket
            .proposal
            .changes
            .iter()
            .find(|c| c.after.id == pending.item)
            .ok_or("equipment focus missing")?;
        let fresh = change.before.is_none()
            && matches!(
                operation.request,
                bace_gameplay_api::InventoryRequest::SplitToWield { .. }
            );
        let old = match change.before.as_ref().map(|i| i.place) {
            Some(ItemPlace::Contained { equipped, .. }) => equipped,
            None if fresh => 0,
            _ => return Err("equipment before placement missing".into()),
        };
        let (container, slot, new) = match change.after.place {
            ItemPlace::Contained {
                container,
                slot,
                equipped,
            } => (container, slot, equipped),
            _ => return Err("equipment after placement missing".into()),
        };
        let moved = old != 0 && new != 0;
        let dat = pending
            .prepared
            .as_ref()
            .and_then(|p| p.equipment_dat.as_ref())
            .ok_or("equipment spell DAT missing")?;
        let mut updates = Vec::new();
        let mut removals = Vec::new();
        let mut magic_order = Vec::new();
        for event in patch.registries.iter().flat_map(|r| &r.events) {
            match event {
                bace_simulation::MagicEvent::Enchantment {
                    actor: target,
                    entry,
                } if *target == actor => {
                    let definition = crate::player_assets::player_enchantment_definition(
                        entry.spell,
                        &dat.spells,
                        &self.assets.spell_rows,
                    )
                    .ok_or("equipment enchantment definition missing")?;
                    let projection = crate::enchantment_saves::prepare_enchantment_projection(
                        entry,
                        Some(definition),
                    )
                    .map_err(|e| e.to_string())?;
                    let registry = bace_replication::project_enchantments(&[projection], 1)
                        .map_err(|e| format!("equipment enchantment: {e:?}"))?;
                    let value = registry
                        .additive
                        .into_iter()
                        .chain(registry.multiplicative)
                        .chain(registry.cooldown)
                        .chain(registry.vitae)
                        .next()
                        .ok_or("equipment enchantment empty")?;
                    magic_order.push((true, updates.len()));
                    updates.push(value);
                }
                bace_simulation::MagicEvent::EnchantmentsRemoved {
                    actor: target,
                    entries,
                } if *target == actor => {
                    let entries = entries
                        .iter()
                        .map(|(s, l)| {
                            Ok((
                                u16::try_from(*s).map_err(|_| "equipment spell overflow")?,
                                *l,
                            ))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    magic_order.push((false, removals.len()));
                    removals.push(entries);
                }
                // Item-local registries have no private character enchantment icon.
                bace_simulation::MagicEvent::Enchantment { .. }
                | bace_simulation::MagicEvent::EnchantmentsRemoved { .. } => {}
                _ => return Err("unsupported equipment effect publication".into()),
            }
        }
        let mut steps = Vec::new();
        let mut public = Vec::new();
        if fresh {
            // ACE's carried SplitToWield sends source SetStackSize before the
            // newly constructed object enters the equip path.
            for stack in &operation.ticket.proposal.changes {
                if stack
                    .before
                    .as_ref()
                    .is_some_and(|before| before.stack != stack.after.stack)
                {
                    steps.push(P::Stack {
                        item: stack.after.id,
                        quantity: stack.after.stack,
                        value: stack
                            .after
                            .stack
                            .checked_mul(stack.after.unit_value)
                            .ok_or("equipment source stack value overflow")?,
                    });
                }
            }
            let child = visibility
                .descriptions
                .iter()
                .find(|d| d.object_id == pending.item.0)
                .ok_or("fresh equipment description missing")?;
            steps.push(P::Create(child));
        }
        if moved {
            steps.push(P::Property {
                item: pending.item,
                property: 10,
                value: PropertyValue::Int(new as i32),
            });
            steps.push(P::Event(InventoryEvent::Wield {
                object_id: pending.item.0,
                location: new,
            }));
        }
        if new != 0
            && let Some(child) = visibility
                .descriptions
                .iter()
                .find(|d| d.object_id == pending.item.0)
        {
            let parent = child
                .physics
                .options
                .parent
                .ok_or("equipment child parent missing")?;
            public.push(steps.len());
            steps.push(P::Parent {
                parent: actor,
                item: pending.item,
                location: parent.location,
                placement: child
                    .physics
                    .options
                    .movement
                    .as_ref()
                    .and_then(|m| {
                        if let bace_wire::PhysicsMovement::AnimationFrame(v) = m {
                            Some(*v)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0),
            });
        }
        if !moved {
            public.push(steps.len());
            steps.push(P::Appearance {
                item: actor,
                model: &visibility.model,
            });
        }
        if !moved {
            steps.push(P::Property {
                item: pending.item,
                property: 3,
                value: PropertyValue::InstanceId(0),
            });
            steps.push(P::Property {
                item: pending.item,
                property: 10,
                value: PropertyValue::Int(0),
            });
            if new != 0 {
                steps.push(P::Event(InventoryEvent::Wield {
                    object_id: pending.item.0,
                    location: new,
                }));
            } else {
                steps.push(P::Pickup(pending.item));
            }
        }
        if (old | new) & 0x03700000 != 0 {
            public.push(steps.len());
        }
        steps.push(P::Effect(bace_wire::CombatEffect::Sound {
            object_id: actor.0,
            sound_id: if new != 0 { 0x8c } else { 0x8d },
            volume: 1.0,
        }));
        if new == 0 {
            steps.push(P::Event(InventoryEvent::PutInContainer {
                object_id: pending.item.0,
                container_id: container.0,
                placement: i32::try_from(slot).map_err(|_| "equipment pack slot")?,
                container_type: 0,
            }));
        }
        if let Some(mode) = operation.equipment_mode_after {
            steps.push(P::PrivateProperty {
                property: 40,
                value: PropertyValue::Int(mode as i32),
            });
        }
        let mut vital_steps = Vec::new();
        let vitals = operation
            .equipment_vitals
            .as_ref()
            .ok_or("equipment vital companion missing")?;
        if let Some(health) = vitals.gear_health {
            vital_steps.push(P::PrivateProperty {
                property: 379,
                value: PropertyValue::Int(health as i32),
            });
        }
        for (index, vital) in [2, 4, 6].into_iter().enumerate() {
            if vitals.before[index] != vitals.after[index]
                || index == 0 && vitals.gear_health.is_some()
            {
                vital_steps.push(P::Vital {
                    vital,
                    current: vitals.after[index],
                });
            }
        }
        if new != 0 {
            steps.append(&mut vital_steps);
        }
        for (update, index) in magic_order {
            steps.push(P::Magic(if update {
                MagicEvent::UpdateEnchantment(&updates[index])
            } else {
                MagicEvent::RemoveMultiple(&removals[index])
            }));
        }
        for message in &patch.activation_messages {
            steps.push(match message {
                ActivationMessage::Error(error) => {
                    P::Simple(bace_wire::SimpleGameEvent::WeenieError(*error))
                }
                ActivationMessage::ErrorWithString { error, text } => {
                    P::Group(bace_wire::GroupEvent::ErrorWithString { code: *error, text })
                }
                ActivationMessage::Transient(text) => {
                    P::Social(bace_wire::SocialEvent::Transient(text))
                }
            });
        }
        steps.append(&mut vital_steps);
        let replica = self
            .players
            .replication(actor)
            .ok_or("equipment replication missing")?;
        if replica.binding != operation.binding || replica.key != key {
            return Err("equipment output binding mismatch".into());
        }
        let mut limits = output::batch_limits(self.limits.message_bytes);
        limits.max_bytes = 1024 * 1024;
        let batch = replica
            .events
            .project_inventory_with_actor(
                operation.binding,
                &steps,
                &mut replica.item_properties,
                Some(&mut replica.properties),
                output::object_limits(self.limits.message_bytes),
                limits,
            )
            .map_err(|e| format!("equipment publication: {e:?}"))?;
        let observers = public
            .into_iter()
            .map(|i| batch.messages[i].clone())
            .collect::<Vec<_>>();
        // Both lanes were capacity checked before any sequence changed.
        self.retain_observer_messages(vec![(actor, observers)])?;
        self.mark_last_observer_visibility_reset();
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|m| (m.queue, m.bytes))
                    .collect(),
            });
        self.inventory.pending = None;
        Ok(())
    }
    fn prepare_equipment_visibility(&mut self) -> Result<EquipmentVisibilityUpdate, String> {
        let p = self
            .inventory
            .pending
            .as_ref()
            .ok_or("equipment output missing")?;
        let c = p
            .completion
            .as_ref()
            .ok_or("equipment completion missing")?;
        let actor = p.binding.actor;
        let operation = &c.work.operation;
        let player = self
            .online_saves
            .baseline(actor.0)
            .ok_or("equipment committed actor baseline missing")?
            .0;
        let items = self.online_saves.equipped_inventory_baselines(actor.0);
        if items.len() > 64 {
            return Err("equipment roster capacity".into());
        }
        let equipment = items
            .iter()
            .map(|i| {
                (
                    i.entity.object_id,
                    &i.entity.state,
                    match i.placement {
                        bace_storage_codec::ItemPlacementV2::Contained { equipped, .. } => equipped,
                        _ => 0,
                    },
                )
            })
            .collect::<Vec<_>>();
        let appearance = p
            .prepared
            .as_ref()
            .and_then(|p| p.appearance.as_ref())
            .ok_or("equipment appearance closure missing")?;
        let chargen = self
            .sessions
            .get(&p.key)
            .and_then(|s| s.loading.as_ref())
            .and_then(|l| l.character_assets.as_ref())
            .ok_or("equipment chargen missing")?;
        let assets = appearance.borrowed(chargen.char_gen());
        let (children, attachments) =
            crate::player_entry::prepare_entry_attachments(actor.0, &equipment, false)?;
        let model = crate::player_entry::prepare_player_model(
            &player.player.entity.state,
            &equipment.iter().map(|(_, s, _)| *s).collect::<Vec<_>>(),
            crate::player_entry::PlayerAppearanceOptions {
                show_helm: player.player.metadata.options2 & 0x100000 != 0,
                show_cloak: player.player.metadata.options2 & 0x800000 != 0,
                default_hair_texture: player.player.metadata.default_hair_texture,
                hair_texture: player.player.metadata.hair_texture,
            },
            &assets,
        )?
        .model;
        let replica = self
            .players
            .replication(actor)
            .ok_or("equipment replica missing")?;
        let mut descriptions = Vec::new();
        for attachment in attachments.iter().filter(|a| a.parent.is_some()) {
            let source = &items
                .iter()
                .find(|i| i.entity.object_id == attachment.item)
                .ok_or("equipment attachment source missing")?
                .entity
                .state;
            let sequences = replica
                .item_properties
                .get(&EntityId(attachment.item))
                .ok_or("equipment child sequence owner missing")?;
            let mut state = output::contained_state(source, sequences);
            state.parent = attachment.parent;
            state.movement = Some(bace_wire::PhysicsMovement::AnimationFrame(
                attachment.placement,
            ));
            if attachment.item == p.item.0 {
                state.sequences.position = state.sequences.position.wrapping_add(1);
            }
            descriptions.push(Arc::new(crate::player_entry::prepare_entry_object(
                attachment.item,
                source,
                crate::player_entry::prepare_item_model(source, &assets)?,
                state,
            )?));
        }
        Ok(EquipmentVisibilityUpdate {
            actor,
            operation: operation.ticket.operation,
            before_revision: operation.actor_revision,
            after_revision: operation
                .equipment_vitals
                .as_ref()
                .ok_or("equipment vital revision missing")?
                .after_revision,
            incarnation: p.key.generation,
            instance_sequence: replica.properties.current(SequenceKind::ObjectInstance, 0),
            model,
            children,
            descriptions,
        })
    }
}
