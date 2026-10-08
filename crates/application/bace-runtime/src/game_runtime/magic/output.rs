//! Canonical private magic projection and explicit public projectile obligations.
use super::*;
use bace_replication::{BatchLimits, InventoryProjection, SessionBatch};
use bace_simulation::MagicEvent;
#[cfg(test)]
mod tests;
/// Fixed-size public owner event retained until the unified accepted visibility
/// producer has taken its exact creation/removal/script obligation.
#[derive(Clone)]
pub struct MagicObserverWork {
    pub sequence: u64,
    pub event: MagicEvent,
    retained_bytes: usize,
}
impl GameRuntime {
    pub fn pending_magic_observers(&self) -> Option<&MagicObserverWork> {
        self.magic.observers.front()
    }
    pub fn acknowledge_magic_observers(&mut self, sequence: u64) -> Result<(), String> {
        if self
            .magic
            .observers
            .front()
            .is_none_or(|w| w.sequence != sequence)
        {
            return Err("magic observer correlation".into());
        }
        let retained_bytes = self
            .magic
            .observers
            .front()
            .ok_or("magic observer owner missing")?
            .retained_bytes;
        let remaining = self
            .magic
            .observer_bytes
            .checked_sub(retained_bytes)
            .ok_or("magic observer byte accounting")?;
        self.magic.observers.pop_front();
        self.magic.observer_bytes = remaining;
        Ok(())
    }
    pub(super) fn poll_magic_output(&mut self, _unix: u64) -> Result<(), String> {
        self.project_magic_components()?;
        for _ in 0..self.limits.work_per_poll {
            if self.magic.event.is_none() {
                self.magic.event = self.simulation.magic_events().try_recv().ok();
            }
            let Some(event) = self.magic.event.clone() else {
                break;
            };
            if !self.project_magic_event(&event)? {
                break;
            }
            self.magic.event = None;
        }
        if self.magic.event.is_some() {
            return Ok(());
        }
        let keys: Vec<_> = self
            .magic
            .pending
            .iter()
            .filter(|(key, p)| {
                matches!(p.phase, Phase::Terminal(_))
                    && !self.magic.resources.owns_session(**key)
                    && !self.magic.cancellations.contains_key(*key)
            })
            .take(self.limits.work_per_poll)
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            if self.sessions.get(&key).is_some_and(|s| s.disconnected) {
                self.magic.pending.remove(&key);
                continue;
            }
            if self.network_output.len() >= self.limits.messages {
                break;
            }
            let p = &self.magic.pending[&key];
            let Phase::Terminal(outcome) = &p.phase else {
                unreachable!()
            };
            let replica = self
                .players
                .replication(p.context.actor)
                .ok_or("cast terminal has no replication owner")?;
            if replica.key != key || replica.binding != binding(p.context) {
                return Err("cast terminal binding mismatch".into());
            }
            let batch = replica
                .events
                .project_cast_outcome(replica.binding, outcome, limits(self.limits.message_bytes))
                .map_err(|e| format!("cast terminal projection: {e:?}"))?;
            queue(self, key, batch)?;
            self.magic.pending.remove(&key);
        }
        Ok(())
    }
    fn project_magic_event(&mut self, event: &MagicEvent) -> Result<bool, String> {
        match event {
            MagicEvent::ComponentsRequired {
                actor,
                cast,
                consumed,
                ..
            } => {
                if !consumed.is_empty() {
                    return self.queue_magic_resources(*actor, *cast);
                }
            }
            MagicEvent::PortalRequired { .. } => {
                if self.magic.portals.len() >= 64 {
                    return Ok(false);
                }
                self.magic.portals.push_back(event.clone());
            }
            MagicEvent::ProjectileCreated { .. }
            | MagicEvent::ProjectileExploded { .. }
            | MagicEvent::ProjectileRemoved { .. }
            | MagicEvent::TargetRejected { .. } => {
                let retained_bytes = observer_bytes(event)?;
                if self.magic.observers.len() >= 128
                    || retained_bytes
                        > (16 * 1024 * 1024usize).saturating_sub(self.magic.observer_bytes)
                {
                    return Ok(false);
                }
                let sequence = self
                    .magic
                    .next_observer
                    .checked_add(1)
                    .ok_or("magic observer sequence exhausted")?;
                self.magic.observers.push_back(MagicObserverWork {
                    sequence,
                    event: event.clone(),
                    retained_bytes,
                });
                self.magic.observer_bytes += retained_bytes;
                self.magic.next_observer = sequence;
            }
            MagicEvent::Fizzle {
                actor,
                intensity,
                movement_incarnation,
            } => {
                let private = if let Some(incarnation) = movement_incarnation {
                    self.players
                        .replication(*actor)
                        .filter(|r| {
                            r.key.generation == *incarnation
                                && self.sessions.get(&r.key).is_some_and(|s| !s.disconnected)
                        })
                        .map(|r| r.key)
                } else {
                    None
                };
                let messages = private
                    .map(|key| {
                        let messages = [7, 26]
                            .into_iter()
                            .map(|chat_type| {
                                bace_wire::ChatMessage::System {
                                    text: "Your movement disrupted spell casting!",
                                    chat_type,
                                }
                                .encode()
                                .map_err(|e| e.to_string())
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok::<_, String>((key, messages))
                    })
                    .transpose()?;
                let bytes = bace_wire::CombatEffect::Script {
                    object_id: actor.0,
                    script_id: 0x51,
                    speed: *intensity,
                }
                .encode(1024, self.limits.message_bytes)
                .map_err(|e| e.to_string())?;
                if !self.observer_room(1, bytes.len())
                    || messages.is_some() && self.network_output.len() >= self.limits.messages
                {
                    return Ok(false);
                }
                self.retain_observer_messages(vec![(
                    *actor,
                    vec![bace_replication::ReplicationMessage { queue: 10, bytes }],
                )])?;
                if let Some((key, messages)) = messages {
                    self.network_output.push_back(NetworkCommand::SendBatch {
                        key,
                        queue: 9,
                        messages,
                    });
                }
            }
            MagicEvent::Vital {
                actor,
                vital,
                after,
                revision,
                incarnation,
                ..
            } => {
                let Some(replica) = self.players.replication(*actor) else {
                    return Ok(true);
                };
                if replica.key.generation != *incarnation {
                    return Ok(true);
                }
                if self
                    .sessions
                    .get(&replica.key)
                    .is_some_and(|s| s.disconnected)
                {
                    return Ok(true);
                }
                if self.network_output.len() >= self.limits.messages {
                    return Ok(false);
                }
                let index = match vital {
                    bace_entity::EntityVital::Health => 0,
                    bace_entity::EntityVital::Stamina => 1,
                    bace_entity::EntityVital::Mana => 2,
                };
                if replica.vital_revisions[index].is_some_and(|seen| *revision <= seen) {
                    return Ok(true);
                }
                let key = replica.key;
                let id = match vital {
                    bace_entity::EntityVital::Health => 2,
                    bace_entity::EntityVital::Stamina => 4,
                    bace_entity::EntityVital::Mana => 6,
                };
                let batch = bace_replication::cast_output::project_magic_vital(
                    replica.binding,
                    id,
                    *after,
                    &mut replica.properties,
                    limits(self.limits.message_bytes),
                )
                .map_err(|e| format!("magic vital projection: {e:?}"))?;
                queue(self, key, batch)?;
                self.players
                    .replication(*actor)
                    .ok_or("magic vital owner retired during projection")?
                    .vital_revisions[index] = Some(*revision);
            }
            MagicEvent::Enchantment { actor, entry } => {
                let Some((key, table)) = self.sessions.iter().find_map(|(key, s)| {
                    s.loading
                        .as_ref()
                        .filter(|l| l.loaded.binding.actor == *actor)
                        .and_then(|l| l.spell_table.clone())
                        .map(|t| (*key, t))
                }) else {
                    return Ok(true);
                };
                if self.sessions[&key].disconnected {
                    return Ok(true);
                }
                if self.network_output.len() >= self.limits.messages {
                    return Ok(false);
                }
                let definition = crate::player_assets::player_enchantment_definition(
                    entry.spell,
                    &table,
                    &self.assets.spell_rows,
                );
                let projection =
                    crate::enchantment_saves::prepare_enchantment_projection(entry, definition)
                        .map_err(|e| e.to_string())?;
                let registry = bace_replication::project_enchantments(&[projection], 1)
                    .map_err(|e| e.to_string())?;
                let wire = registry
                    .vitae
                    .into_iter()
                    .chain(registry.additive)
                    .chain(registry.multiplicative)
                    .chain(registry.cooldown)
                    .next()
                    .ok_or("empty enchantment projection")?;
                let replica = self
                    .players
                    .replication(*actor)
                    .ok_or("enchantment replication missing")?;
                let batch = replica
                    .events
                    .project_magic(
                        replica.binding,
                        &[bace_wire::MagicEvent::UpdateEnchantment(&wire)],
                        1,
                        limits(self.limits.message_bytes),
                    )
                    .map_err(|e| format!("enchantment projection: {e:?}"))?;
                queue(self, key, batch)?;
            }
            MagicEvent::EnchantmentExpired { .. } => return self.project_magic_expiry(event),
            MagicEvent::EnchantmentsRemoved { actor, entries } => {
                let Some(replica) = self.players.replication(*actor) else {
                    return Ok(true);
                };
                if self
                    .sessions
                    .get(&replica.key)
                    .is_some_and(|s| s.disconnected)
                {
                    return Ok(true);
                }
                if self.network_output.len() >= self.limits.messages {
                    return Ok(false);
                }
                let wire: Vec<_> = entries
                    .iter()
                    .map(|(spell, layer)| u16::try_from(*spell).map(|s| (s, *layer)))
                    .collect::<Result<_, _>>()
                    .map_err(|_| "removed spell ID overflow")?;
                let key = replica.key;
                let batch = replica
                    .events
                    .project_magic(
                        replica.binding,
                        &[bace_wire::MagicEvent::RemoveMultiple(&wire)],
                        4096,
                        limits(self.limits.message_bytes),
                    )
                    .map_err(|e| format!("enchantment removal: {e:?}"))?;
                queue(self, key, batch)?;
            }
            // F74C accepted motion/ObjectView is the sole presentation owner.
            // Client animation playback executes DAT sound/particle hooks; these
            // owner diagnostics must not duplicate those effects or counters.
            MagicEvent::Motion { .. }
            | MagicEvent::MotionStopped { .. }
            | MagicEvent::Turning { .. }
            | MagicEvent::MotionHook { .. } => {}
        }
        Ok(true)
    }
    fn project_magic_components(&mut self) -> Result<(), String> {
        let Some(done) = self.magic.resources.completion.as_ref() else {
            return Ok(());
        };
        if !done.committed || self.sessions.get(&done.key).is_some_and(|s| s.disconnected) {
            self.magic.resources.completion = None;
            return Ok(());
        }
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let key = done.key;
        let replica = self
            .players
            .replication(done.binding.actor)
            .ok_or("component replication owner missing")?;
        if replica.key != key || replica.binding != done.binding {
            return Err("component replication binding mismatch".into());
        }
        let steps: Vec<_> = done
            .ticket
            .inventory
            .proposal
            .changes
            .iter()
            .map(|c| {
                if c.after.place == bace_inventory::ItemPlace::Removed {
                    Ok(InventoryProjection::Remove(c.after.id))
                } else {
                    Ok(InventoryProjection::Stack {
                        item: c.after.id,
                        quantity: c.after.stack,
                        value: c
                            .after
                            .unit_value
                            .checked_mul(c.after.stack)
                            .ok_or("component value overflow")?,
                    })
                }
            })
            .collect::<Result<_, String>>()?;
        let objects = bace_wire::ObjectCodecLimits {
            max_message_bytes: self.limits.message_bytes,
            max_model_entries: 255,
            max_children: 128,
            max_restrictions: 1024,
            max_motion_commands: 32,
            max_string_bytes: 1024,
        };
        let batch = replica
            .events
            .project_inventory(
                done.binding,
                &steps,
                &mut replica.item_properties,
                objects,
                limits(self.limits.message_bytes),
            )
            .map_err(|e| format!("component projection: {e:?}"))?;
        queue(self, key, batch)?;
        self.magic.resources.completion = None;
        Ok(())
    }
}
fn queue(runtime: &mut GameRuntime, key: SessionKey, batch: SessionBatch) -> Result<(), String> {
    if batch.messages.is_empty() {
        return Ok(());
    }
    if batch.messages.iter().any(|m| m.queue != 9) {
        return Err("private magic queue mismatch".into());
    }
    runtime.network_output.push_back(NetworkCommand::SendBatch {
        key,
        queue: 9,
        messages: batch.messages.into_iter().map(|m| m.bytes).collect(),
    });
    Ok(())
}
fn limits(max: usize) -> BatchLimits {
    BatchLimits {
        max_messages: 4096,
        max_bytes: 64 * 1024 * 1024,
        max_message_bytes: max,
        max_string_bytes: 1024,
    }
}

fn observer_bytes(event: &MagicEvent) -> Result<usize, String> {
    let extra = match event {
        MagicEvent::ProjectileCreated {
            launch: Some(launch),
            ..
        } => {
            if launch.observers.len() > 4096 {
                return Err("spell launch observer capacity".into());
            }
            launch
                .observers
                .len()
                .checked_mul(std::mem::size_of::<
                    bace_gameplay_api::visibility::ProjectileLaunchObserver,
                >())
                .ok_or("spell launch byte overflow")?
        }
        MagicEvent::TargetRejected {
            notice: Some(notice),
            ..
        } => {
            if notice.source_name.len() > 1024 || notice.target_name.len() > 1024 {
                return Err("resistance notice capacity".into());
            }
            notice.source_name.len() + notice.target_name.len()
        }
        _ => 0,
    };
    extra
        .checked_add(std::mem::size_of::<MagicObserverWork>() + 512)
        .ok_or_else(|| "magic observer byte overflow".into())
}
