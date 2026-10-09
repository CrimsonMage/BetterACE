//! The committed row supplies gameplay qualities and placement. Verified DATs
//! supply the model; canonical replication supplies all sequences.
use super::*;
use crate::{
    inventory_service::project_inventory_completion,
    player_entry::{EntryObjectState, prepare_entry_object, prepare_item_model},
};
use bace_inventory::ItemPlace;
use bace_replication::{BatchLimits, Sequences};
use bace_storage_codec::ItemSaveV4;
use bace_wire::{ObjectCodecLimits, PhysicsMovement};
impl GameRuntime {
    pub(super) fn project_inventory(&mut self) -> Result<(), String> {
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        if let Some(pending) = self.inventory.pending.as_mut()
            && let Some(motion) = pending.motions.front()
        {
            let replica = self
                .players
                .replication(pending.binding.actor)
                .ok_or("inventory motion replication owner missing")?;
            if replica.key != pending.key || motion.actor != pending.binding.actor {
                return Err("inventory motion binding mismatch".into());
            }
            let view = bace_wire::MovementDescription {
                autonomous: false,
                motion_flags: 0,
                current_style: motion.style as u16,
                body: bace_wire::MotionBody::State {
                    state: bace_wire::InterpretedMotion {
                        current_style: Some(motion.style as u16),
                        forward_command: Some(motion.command.unwrap_or(0x41000003) as u16),
                        ..Default::default()
                    },
                    sticky_object: None,
                },
            };
            let output = bace_replication::project_server_motion(
                motion.actor.0,
                &view,
                &mut replica.properties,
                batch_limits(self.limits.message_bytes),
            )
            .map_err(|e| format!("inventory motion projection: {e:?}"))?;
            self.network_output.push_back(NetworkCommand::Send {
                key: pending.key,
                queue: output.queue,
                bytes: output.bytes,
            });
            pending.motions.pop_front();
            return Ok(());
        }
        if self
            .inventory
            .pending
            .as_ref()
            .and_then(|p| p.completion.as_ref())
            .is_some_and(|c| c.committed && c.work.operation.equipment.is_some())
        {
            return self.project_inventory_equipment();
        }
        let Some(pending) = self.inventory.pending.as_ref() else {
            return Ok(());
        };
        if pending.rejected {
            let replica = self
                .players
                .replication(pending.binding.actor)
                .ok_or("inventory rejection replication owner missing")?;
            if replica.key != pending.key || replica.binding != pending.binding {
                return Err("inventory rejection session mismatch".into());
            }
            let event = bace_wire::InventoryEvent::SaveFailed {
                item_id: pending.item.0,
                error: 0,
            };
            let batch = replica
                .events
                .project_inventory(
                    pending.binding,
                    &[bace_replication::InventoryProjection::Event(event)],
                    &mut replica.item_properties,
                    object_limits(self.limits.message_bytes),
                    batch_limits(self.limits.message_bytes),
                )
                .map_err(|e| format!("inventory rejection projection: {e:?}"))?;
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch {
                    key: pending.key,
                    messages: batch
                        .messages
                        .into_iter()
                        .map(|m| (m.queue, m.bytes))
                        .collect(),
                });
            self.inventory.pending = None;
            return Ok(());
        }
        let Some(completion) = &pending.completion else {
            return Ok(());
        };
        if !completion.committed {
            self.inventory
                .pending
                .as_mut()
                .expect("pending completion")
                .rejected = true;
            return Ok(());
        }
        let key = pending.key;
        let binding = pending.binding;
        let loading = self
            .sessions
            .get(&key)
            .and_then(|s| s.loading.as_ref())
            .ok_or("inventory output session metadata missing")?;
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("inventory canonical replication owner missing")?;
        if replica.key != key || replica.binding != binding {
            return Err("inventory output session binding mismatch".into());
        }
        let mut created = BTreeMap::new();
        let mut fresh_sequences = Vec::new();
        let mut removals = Vec::new();
        for change in &completion.work.operation.ticket.proposal.changes {
            if change.after.place == ItemPlace::Removed {
                removals.push(change.after.id);
            }
            if change.before.is_some() && replica.item_properties.contains_key(&change.after.id) {
                continue;
            }
            if change.after.place == ItemPlace::Removed {
                continue;
            }
            let id = change.after.id;
            if replica.item_properties.contains_key(&id) {
                return Err("fresh split sequence owner already exists".into());
            }
            let row = completion
                .snapshots
                .iter()
                .find(|row| row.object_id == id.0)
                .ok_or("fresh committed split row missing")?;
            let (saved, _) = crate::game_inventory::decode_inventory_item(&row.bytes, None)
                .map_err(|e| e.to_string())?;
            if !matches!(
                change.after.place,
                ItemPlace::Contained { equipped: 0, .. } | ItemPlace::World
            ) {
                return Err(
                    "fresh inventory description requires accepted world/wield attachment".into(),
                );
            }
            let appearance = pending
                .prepared
                .as_ref()
                .and_then(|p| p.appearance.as_ref())
                .ok_or("fresh split appearance closure missing")?;
            let character = loading
                .character_assets
                .as_ref()
                .ok_or("entry chargen metadata missing")?;
            let sequences =
                Sequences::new(256).map_err(|e| format!("fresh sequence capacity: {e:?}"))?;
            let object = prepare_entry_object(
                id.0,
                &saved.entity.state,
                prepare_item_model(
                    &saved.entity.state,
                    &appearance.borrowed(character.char_gen()),
                )?,
                committed_state(&saved, &sequences)?,
            )?;
            created.insert(id, object);
            fresh_sequences.push((id, sequences));
        }
        if replica.item_properties.len() + fresh_sequences.len() > 1023
            || removals
                .iter()
                .any(|id| !replica.item_properties.contains_key(id))
        {
            return Err("inventory sequence identity/capacity".into());
        }
        let max = self.limits.message_bytes;
        let objects = object_limits(max);
        let limits = batch_limits(max);
        let fresh_ids: Vec<_> = fresh_sequences.iter().map(|(id, _)| *id).collect();
        for (id, sequences) in fresh_sequences {
            replica.item_properties.insert(id, sequences);
        }
        let projected = project_inventory_completion(
            completion,
            &mut replica.events,
            &mut replica.item_properties,
            &created,
            objects,
            limits,
        );
        let batch = match projected {
            Ok(Some(batch)) => batch,
            other => {
                for id in fresh_ids {
                    replica.item_properties.remove(&id);
                }
                return Err(format!("inventory projection: {other:?}"));
            }
        };
        // Encoding validated the complete batch before advancing any counter.
        // Queue capacity was reserved above; no fallible work follows adoption.
        for id in removals {
            replica.item_properties.remove(&id);
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
        self.inventory.pending = None;
        Ok(())
    }
}
pub(in crate::game_runtime) fn contained_state(
    source: &bace_content::WeenieV1,
    sequences: &Sequences,
) -> EntryObjectState {
    EntryObjectState {
        is_player: false,
        is_creature: false,
        physics_state: source
            .properties
            .ints
            .iter()
            .find(|p| p.id == 93)
            .map_or(0x00400c08, |p| p.value as u32),
        position: None,
        movement: Some(PhysicsMovement::AnimationFrame(101)),
        parent: None,
        children: vec![],
        velocity: [0.; 3],
        acceleration: [0.; 3],
        omega: [0.; 3],
        sequences: crate::player_entry::physics_sequences(sequences),
        admin_vision: false,
        change_no_draw: false,
        cloak_status: 0,
    }
}

pub(super) fn object_limits(max: usize) -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_message_bytes: max,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 1024,
    }
}
pub(super) fn batch_limits(max: usize) -> BatchLimits {
    BatchLimits {
        max_messages: 4096,
        max_bytes: 64 * 1024 * 1024,
        max_message_bytes: max,
        max_string_bytes: 1024,
    }
}

pub(in crate::game_runtime) fn committed_state(
    saved: &ItemSaveV4,
    sequences: &Sequences,
) -> Result<EntryObjectState, String> {
    let mut state = contained_state(&saved.entity.state, sequences);
    match &saved.placement {
        bace_storage_codec::ItemPlacementV2::Contained { equipped: 0, .. } => {}
        bace_storage_codec::ItemPlacementV2::World(p) => {
            state.position = Some(bace_wire::WirePosition {
                cell: p.obj_cell_id,
                origin: [p.position_x, p.position_y, p.position_z],
                rotation: [p.rotation_w, p.rotation_x, p.rotation_y, p.rotation_z],
            });
        }
        _ => return Err("inventory description requires accepted placement/attachment".into()),
    }
    Ok(state)
}
