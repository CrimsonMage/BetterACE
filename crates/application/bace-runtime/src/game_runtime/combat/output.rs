//! Ordered physical notifications and accepted private health. The generic
//! health stream and magic stream share one per-vital revision fence.
use super::*;
use bace_replication::{BatchLimits, InventoryProjection, SessionBatch};
use bace_simulation::{CombatEvent as E, PhysicalCombatEvent as P};
impl GameRuntime {
    pub(super) fn poll_combat_output(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            if self.combat.outcome.is_none() {
                self.combat.outcome = self.simulation.combat_outcomes().try_recv().ok();
            }
            let Some(outcome) = self.combat.outcome.as_ref() else {
                break;
            };
            let key = self
                .combat
                .pending
                .iter()
                .find_map(|(k, c)| (*c == outcome.context).then_some(*k))
                .ok_or("combat outcome without exact accepted intent")?;
            if self.network_output.len() >= self.limits.messages {
                break;
            }
            if self.sessions.get(&key).is_some_and(|s| !s.disconnected) {
                let replica = self
                    .players
                    .replication(outcome.context.actor)
                    .ok_or("combat replication owner missing")?;
                if replica.key != key {
                    return Err("combat generation mismatch".into());
                }
                let batch = replica
                    .events
                    .project_combat_outcome(
                        replica.binding,
                        outcome,
                        &mut replica.properties,
                        limits(self.limits.message_bytes),
                    )
                    .map_err(|e| format!("combat projection: {e:?}"))?;
                queue(self, key, batch);
            }
            self.combat.pending.remove(&key);
            self.combat.outcome = None;
        }
        for _ in 0..self.limits.work_per_poll {
            if self.combat.event.is_none() {
                self.combat.event = self.simulation.combat_events().try_recv().ok();
            }
            let Some(event) = self.combat.event else {
                break;
            };
            let (actor, current, revision, incarnation) = match event {
                E::Damage {
                    target,
                    current,
                    revision,
                    target_incarnation,
                    ..
                } => (target, Some(current), revision, target_incarnation),
                E::Finished {
                    actor,
                    actor_incarnation,
                    ..
                } => (actor, None, 0, actor_incarnation),
            };
            if self.network_output.len() >= self.limits.messages {
                break;
            }
            if let Some(replica) = self.players.replication(actor)
                && replica.key.generation == incarnation
                && self
                    .sessions
                    .get(&replica.key)
                    .is_some_and(|s| !s.disconnected)
            {
                let key = replica.key;
                let batch = if let Some(current) = current {
                    if replica.vital_revisions[0].is_some_and(|seen| seen >= revision) {
                        self.combat.event = None;
                        continue;
                    }
                    let batch = bace_replication::cast_output::project_magic_vital(
                        replica.binding,
                        2,
                        current,
                        &mut replica.properties,
                        limits(self.limits.message_bytes),
                    )
                    .map_err(|e| format!("physical health projection: {e:?}"))?;
                    replica.vital_revisions[0] = Some(revision);
                    batch
                } else {
                    replica
                        .events
                        .project_combat(
                            replica.binding,
                            &[bace_wire::CombatEvent::AttackDone(0)],
                            limits(self.limits.message_bytes),
                        )
                        .map_err(|e| format!("scalar attack done: {e:?}"))?
                };
                queue(self, key, batch);
            }
            self.combat.event = None;
        }
        for _ in 0..self.limits.work_per_poll {
            if self.combat.physical.is_none() {
                self.combat.physical = self.simulation.physical_events().try_recv().ok();
            }
            let Some(event) = self.combat.physical.clone() else {
                break;
            };
            if !self.project_physical_event(&event)? {
                break;
            }
            self.combat.physical = None;
            self.combat.physical_recipient = 0;
        }
        Ok(())
    }
    fn private_combat_event(
        &mut self,
        actor: EntityId,
        incarnation: u64,
        event: bace_wire::CombatEvent<'_>,
    ) -> Result<bool, String> {
        let Some(replica) = self.players.replication(actor) else {
            return Ok(true);
        };
        if replica.key.generation != incarnation
            || self
                .sessions
                .get(&replica.key)
                .is_none_or(|s| s.disconnected)
        {
            return Ok(true);
        }
        queue_private_notice(
            replica,
            incarnation,
            event,
            &mut self.network_output,
            self.limits.messages,
            self.limits.message_bytes,
        )
    }
    fn project_physical_event(&mut self, event: &P) -> Result<bool, String> {
        use bace_wire::CombatEvent as W;
        match event {
            P::ModeChanged {
                actor,
                actor_incarnation,
                mode,
            } => {
                let Some(replica) = self
                    .players
                    .replication(*actor)
                    .filter(|r| r.key.generation == *actor_incarnation)
                else {
                    return Ok(true);
                };
                if self
                    .sessions
                    .get(&replica.key)
                    .is_none_or(|s| s.disconnected)
                {
                    return Ok(true);
                }
                if self.network_output.len() >= self.limits.messages {
                    return Ok(false);
                }
                let sequence = replica
                    .properties
                    .current(bace_replication::SequenceKind::PropertyInt, 40)
                    .wrapping_add(1) as u8;
                let bytes = bace_wire::PropertyUpdate {
                    sequence,
                    object_id: None,
                    property: 40,
                    value: bace_wire::PropertyValue::Int(*mode as i32),
                }
                .encode()
                .map_err(|e| format!("automatic mode output: {e:?}"))?;
                replica
                    .properties
                    .advance(bace_replication::SequenceKind::PropertyInt, 40)
                    .map_err(|e| format!("automatic mode counter: {e:?}"))?;
                let key = replica.key;
                self.network_output
                    .push_back(NetworkCommand::SendOrderedBatch {
                        key,
                        messages: vec![(9, bytes)],
                    });
                Ok(true)
            }
            P::AttackDone {
                actor,
                actor_incarnation,
            } => self.private_combat_event(*actor, *actor_incarnation, W::AttackDone(0)),
            P::CommenceAttack {
                actor,
                actor_incarnation,
            } => self.private_combat_event(*actor, *actor_incarnation, W::CommenceAttack),
            P::Fault {
                actor,
                actor_incarnation,
                error,
                ..
            } => self.private_combat_event(
                *actor,
                *actor_incarnation,
                W::AttackDone(bace_replication::combat_actions::combat_error_code(*error)),
            ),
            P::Impact {
                attacker,
                target,
                attacker_incarnation,
                target_incarnation,
                impact,
                attacker_name,
                target_name,
                applied,
                current,
                maximum,
                ..
            } => {
                let damage = |name| bace_wire::DamageNotification {
                    name,
                    damage_type: impact.damage_type,
                    percent: if *maximum == 0 {
                        0.
                    } else {
                        (f64::from(*applied) / f64::from(*maximum)) as f32
                    },
                    damage: *applied,
                    critical: impact.critical,
                    conditions: u64::from(impact.attack_conditions),
                };
                if self.combat.physical_recipient == 0 {
                    let event = if impact.evaded {
                        W::EvadedBy(target_name)
                    } else {
                        W::Attacker(damage(target_name))
                    };
                    if !self.private_combat_event(*attacker, *attacker_incarnation, event)? {
                        return Ok(false);
                    }
                    self.combat.physical_recipient = 1;
                }
                // GDLE's defender hit notification is in the surviving-target
                // branch; the death owner supplies terminal victim presentation.
                if *current == 0 && *applied > 0 && !impact.evaded {
                    return Ok(true);
                }
                let event = if impact.evaded {
                    W::EvadedAttackFrom(attacker_name)
                } else {
                    W::Defender {
                        damage: damage(attacker_name),
                        location: impact.body_part,
                    }
                };
                self.private_combat_event(*target, *target_incarnation, event)
            }
            P::Motion { .. } | P::MotionHook { .. } => Ok(true),
            P::ProjectileCreated { .. }
            | P::ProjectileDestroy { .. }
            | P::ProjectileResting { .. }
            | P::ProjectileRemoved { .. } => self.project_physical_projectile(event),
        }
    }
    pub(super) fn project_ammunition_completion(&mut self) -> Result<(), String> {
        let Some(done) = &self.combat.resources.completion else {
            return Ok(());
        };
        if !done.committed {
            let id = EntityId(done.launch.projectile);
            self.visibility
                .service
                .retire_object(id, 0)
                .map_err(|e| format!("rejected projectile appearance retirement: {e:?}"))?;
            self.combat.projectiles.blueprints.remove(&id);
            let binding = done.binding;
            if !self.private_combat_event(
                binding.actor,
                binding.session.0,
                bace_wire::CombatEvent::AttackDone(
                    bace_replication::combat_actions::combat_error_code(
                        bace_gameplay_api::CombatRejection::Busy,
                    ),
                ),
            )? {
                return Ok(());
            }
            self.combat.resources.completion = None;
            return Ok(());
        }
        let ticket = done
            .ticket
            .as_ref()
            .ok_or("committed ammunition ticket missing")?;
        for change in &ticket.inventory.proposal.changes {
            self.visibility
                .service
                .apply_ammunition_child(done.binding, ticket.inventory.operation, change)
                .map_err(|e| format!("public ammunition child receipt retained: {e:?}"))?;
        }
        let Some(replica) = self
            .players
            .replication(done.binding.actor)
            .filter(|replica| replica.binding == done.binding)
        else {
            // The committed baseline was adopted before the owner acknowledgment.
            // A retired session must not block shutdown or send its old inventory
            // notification to a later admission of the same character.
            self.combat.resources.completion = None;
            return Ok(());
        };
        if self
            .sessions
            .get(&replica.key)
            .is_none_or(|s| s.disconnected)
        {
            self.combat.resources.completion = None;
            return Ok(());
        }
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let ticket = done
            .ticket
            .as_ref()
            .ok_or("committed ammunition ticket missing")?;
        let steps: Vec<_> = ticket
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
                            .ok_or("ammunition value overflow")?,
                    })
                }
            })
            .collect::<Result<_, String>>()?;
        let key = replica.key;
        let batch = replica
            .events
            .project_inventory(
                replica.binding,
                &steps,
                &mut replica.item_properties,
                bace_wire::ObjectCodecLimits {
                    max_message_bytes: self.limits.message_bytes,
                    max_model_entries: 255,
                    max_children: 128,
                    max_restrictions: 1024,
                    max_motion_commands: 32,
                    max_string_bytes: 1024,
                },
                limits(self.limits.message_bytes),
            )
            .map_err(|e| format!("ammunition output retained: {e:?}"))?;
        queue(self, key, batch);
        self.combat.resources.completion = None;
        Ok(())
    }
}
fn limits(max: usize) -> BatchLimits {
    BatchLimits {
        max_messages: 32,
        max_bytes: max,
        max_message_bytes: max,
        max_string_bytes: 1024,
    }
}
fn queue(runtime: &mut GameRuntime, key: SessionKey, batch: SessionBatch) {
    if !batch.messages.is_empty() {
        runtime
            .network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|m| (m.queue, m.bytes))
                    .collect(),
            });
    }
}

fn queue_private_notice(
    replica: &mut crate::player_service::SessionReplication,
    incarnation: u64,
    event: bace_wire::CombatEvent<'_>,
    output: &mut VecDeque<NetworkCommand>,
    capacity: usize,
    maximum_bytes: usize,
) -> Result<bool, String> {
    if replica.key.generation != incarnation {
        return Ok(true);
    }
    if output.len() >= capacity {
        return Ok(false);
    }
    let batch = replica
        .events
        .project_combat(replica.binding, &[event], limits(maximum_bytes))
        .map_err(|e| format!("physical notification: {e:?}"))?;
    if !batch.messages.is_empty() {
        output.push_back(NetworkCommand::SendOrderedBatch {
            key: replica.key,
            messages: batch
                .messages
                .into_iter()
                .map(|m| (m.queue, m.bytes))
                .collect(),
        });
    }
    Ok(true)
}
#[cfg(test)]
mod tests;
