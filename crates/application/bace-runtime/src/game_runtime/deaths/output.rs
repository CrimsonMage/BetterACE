//! Canonical death output. Each retained delivery is acknowledged only after
//! its private and observer packets enter their reliable owners.
use super::*;
mod completed;
mod corpse_location;
mod equipped_corpse;
mod equipped_no_corpse;
mod pk_status;
mod protection;
use bace_replication::InventoryProjection as P;
use bace_replication::{BatchLimits, ReplicationMessage, SessionBatch};
use bace_wire::{CombatEffect, CombatEvent};

struct DeathStartProjection<'a> {
    actor: EntityId,
    accepted: &'a bace_gameplay_api::visibility::AcceptedObjectView,
    num_deaths: u32,
    death_level: Option<u32>,
    vitae_pool: Option<i32>,
    vitae: Option<&'a bace_magic::EnchantmentEntry>,
    purge_bad: bool,
    suicide: bool,
}

impl GameRuntime {
    pub(super) fn register_death_portal_watches(&mut self) -> Result<(), String> {
        let watches = self
            .deaths
            .deliveries
            .iter()
            .filter_map(|delivery| match &delivery.work {
                DeathDeliveryWork::Event(PlayerDeathEvent::Corpse {
                    operation,
                    actor,
                    accepted_player: Ok(accepted),
                    ..
                }) => Some((*actor, *operation, accepted.epoch)),
                DeathDeliveryWork::Event(PlayerDeathEvent::WorldDrops {
                    operation,
                    actor,
                    accepted_player: Ok(accepted),
                    ..
                }) => Some((*actor, *operation, accepted.epoch)),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (actor, operation, epoch) in watches {
            self.watch_death_portal_materialization(actor, operation, epoch)?;
        }
        Ok(())
    }

    pub(super) fn project_death_deliveries(&mut self) -> Result<(), String> {
        if self.visibility.service.pending() {
            return Ok(());
        }
        for _ in 0..self.limits.work_per_poll {
            let Some(delivery) = self.deaths.deliveries.front() else {
                break;
            };
            if let DeathDeliveryWork::Completed {
                completion,
                corpse_visibility,
                world_visibility,
                ..
            } = &delivery.work
            {
                let operation = completion.work.ticket.operation;
                if self.deaths.completed_presentations.len() >= LIMIT
                    || self.deaths.completed_presentations.contains_key(&operation)
                {
                    break;
                }
                let presentation = CompletedPresentation {
                    actor: completion.work.ticket.actor,
                    corpse: completion.work.ticket.corpse,
                    binding: completion.work.binding,
                    corpse_visibility: corpse_visibility.clone(),
                    world_visibility: world_visibility.clone(),
                };
                let sequence = delivery.sequence;
                if !self.project_death_completed_inventory(operation)? {
                    break;
                }
                self.deaths
                    .completed_presentations
                    .insert(operation, presentation);
                self.acknowledge_death_delivery(sequence)?;
                continue;
            }
            if let DeathDeliveryWork::Expired {
                event,
                ticket,
                spill,
            } = &delivery.work
            {
                let (sequence, event, ticket, spill) =
                    (delivery.sequence, *event, ticket.clone(), spill.clone());
                if !self.project_corpse_expired(event, &ticket, spill.as_deref())? {
                    break;
                }
                self.acknowledge_death_delivery(sequence)?;
                continue;
            }
            if let DeathDeliveryWork::Event(PlayerDeathEvent::Started {
                actor,
                accepted,
                num_deaths,
                death_level,
                vitae_pool,
                vitae,
                purge_bad,
                suicide,
                ..
            }) = &delivery.work
            {
                let (
                    sequence,
                    actor,
                    accepted,
                    num_deaths,
                    death_level,
                    vitae_pool,
                    vitae,
                    purge_bad,
                    suicide,
                ) = (
                    delivery.sequence,
                    *actor,
                    accepted.clone(),
                    *num_deaths,
                    *death_level,
                    *vitae_pool,
                    vitae.clone(),
                    *purge_bad,
                    *suicide,
                );
                let Ok(accepted) = accepted else {
                    break;
                };
                if !self.project_death_started(DeathStartProjection {
                    actor,
                    accepted: &accepted,
                    num_deaths,
                    death_level,
                    vitae_pool,
                    vitae: vitae.as_ref(),
                    purge_bad,
                    suicide,
                })? {
                    break;
                }
                self.acknowledge_death_delivery(sequence)?;
                continue;
            }
            if let DeathDeliveryWork::Event(PlayerDeathEvent::Corpse {
                operation,
                corpse,
                accepted_corpse,
                ..
            }) = &delivery.work
            {
                let (sequence, operation, corpse, accepted) = (
                    delivery.sequence,
                    *operation,
                    *corpse,
                    accepted_corpse.clone(),
                );
                let Ok(accepted) = accepted else {
                    break;
                };
                if !self.project_death_corpse(operation, corpse, &accepted)? {
                    break;
                }
                self.acknowledge_death_delivery(sequence)?;
                continue;
            }
            if let DeathDeliveryWork::Event(PlayerDeathEvent::WorldDrops {
                operation,
                actor,
                roots,
                ..
            }) = &delivery.work
            {
                let (sequence, operation, actor, roots) =
                    (delivery.sequence, *operation, *actor, roots.clone());
                if !self.project_death_world_drops(operation, actor, &roots)? {
                    break;
                }
                self.acknowledge_death_delivery(sequence)?;
                continue;
            }
            if let DeathDeliveryWork::Event(PlayerDeathEvent::Respawned {
                operation,
                actor,
                accepted,
                vitals,
                vital_revision,
                ..
            }) = &delivery.work
            {
                let (sequence, operation, actor, accepted, vitals, revision) = (
                    delivery.sequence,
                    *operation,
                    *actor,
                    accepted.clone(),
                    *vitals,
                    *vital_revision,
                );
                let Ok(accepted) = accepted else {
                    break;
                };
                if !self.project_death_respawned(operation, actor, &accepted, vitals, revision)? {
                    break;
                }
                self.acknowledge_death_delivery(sequence)?;
                self.forget_death_portal_materialization(actor, operation, accepted.epoch);
                self.deaths.completed_presentations.remove(&operation);
                continue;
            }
            let protection = match &delivery.work {
                DeathDeliveryWork::Event(PlayerDeathEvent::ProtectionExpired {
                    actor,
                    recipient,
                }) => Some((
                    delivery.sequence,
                    *actor,
                    *recipient,
                    protection::Kind::Expired,
                )),
                DeathDeliveryWork::Event(PlayerDeathEvent::ProtectionDispelled {
                    actor,
                    recipient,
                }) => Some((
                    delivery.sequence,
                    *actor,
                    *recipient,
                    protection::Kind::Dispelled,
                )),
                _ => None,
            };
            if let Some((sequence, actor, recipient, kind)) = protection {
                if !self.project_death_protection_notice(actor, recipient, kind)? {
                    break;
                }
                self.acknowledge_death_delivery(sequence)?;
                continue;
            }
            if let DeathDeliveryWork::Event(PlayerDeathEvent::PkStatus {
                actor,
                status,
                recipient,
            }) = &delivery.work
            {
                let (sequence, actor, status, recipient) =
                    (delivery.sequence, *actor, *status, *recipient);
                if !self.project_death_pk_status(actor, status, recipient)? {
                    break;
                }
                self.acknowledge_death_delivery(sequence)?;
                continue;
            }
            let (sequence, actor, announcement) = match &delivery.work {
                DeathDeliveryWork::Event(PlayerDeathEvent::Prepare {
                    actor,
                    announcement: Some(announcement),
                    ..
                }) => (delivery.sequence, *actor, announcement.clone()),
                // Missing accepted blow metadata and later lifecycle stages
                // retain their distinct presentation obligations.
                _ => break,
            };
            let victim = self.players.replication(actor);
            let victim_recipient = victim
                .filter(|r| self.sessions.get(&r.key).is_some_and(|s| !s.disconnected))
                .map(|r| (r.key, r.binding, r.events.next_sequence()));
            let killer_recipient = announcement
                .last_damager
                .filter(|id| *id != actor)
                .and_then(|id| self.players.replication(id))
                .filter(|r| self.sessions.get(&r.key).is_some_and(|s| !s.disconnected))
                .map(|r| (r.key, r.binding, r.events.next_sequence()));
            if victim_recipient.is_none()
                && self.sessions.values().any(|s| {
                    s.loading
                        .as_ref()
                        .is_some_and(|l| l.loaded.binding.actor == actor && !s.disconnected)
                })
            {
                return Err("death victim has no canonical replication owner".into());
            }
            let private_count = usize::from(victim_recipient.is_some())
                + usize::from(killer_recipient.is_some() && announcement.killer_text.is_some());
            if self.network_output.len() + private_count > self.limits.messages
                || !self.observer_room(1, 8192)
            {
                break;
            }
            let limits = BatchLimits {
                max_messages: 1,
                max_bytes: self.limits.message_bytes,
                max_message_bytes: self.limits.message_bytes,
                max_string_bytes: 4096,
            };
            // Preflight all codecs and exact event counters before advancing any
            // canonical sequencer or retaining a visibility fanout.
            if let Some((_, binding, counter)) = victim_recipient {
                CombatEvent::VictimNotification(&announcement.victim_text)
                    .encode(binding.actor.0, counter, 4096, self.limits.message_bytes)
                    .map_err(|error| format!("death victim codec: {error:?}"))?;
            }
            if let (Some((_, binding, counter)), Some(text)) =
                (killer_recipient, announcement.killer_text.as_ref())
            {
                CombatEvent::KillerNotification(text)
                    .encode(binding.actor.0, counter, 4096, self.limits.message_bytes)
                    .map_err(|error| format!("death killer codec: {error:?}"))?;
            }
            let public = CombatEffect::PlayerKilled {
                text: &announcement.broadcast_text,
                victim_id: actor.0,
                killer_id: announcement.last_damager.map_or(0, |id| id.0),
            }
            .encode(4096, self.limits.message_bytes)
            .map_err(|error| format!("death broadcast codec: {error:?}"))?;
            if !self.observer_room(1, public.len()) {
                break;
            }
            if let Some((key, binding, _)) = victim_recipient {
                let replica = self
                    .players
                    .replication(actor)
                    .ok_or("death victim vanished before output")?;
                let batch = replica
                    .events
                    .project_combat(
                        binding,
                        &[CombatEvent::VictimNotification(&announcement.victim_text)],
                        limits,
                    )
                    .map_err(|error| format!("death victim projection: {error:?}"))?;
                self.network_output.push_back(
                    crate::game_messages::session_batch_command(key, batch)
                        .map_err(|error| error.to_string())?,
                );
            }
            if let (Some((key, binding, _)), Some(text)) =
                (killer_recipient, announcement.killer_text.as_ref())
            {
                let replica = self
                    .players
                    .replication(binding.actor)
                    .ok_or("death killer vanished before output")?;
                let batch = replica
                    .events
                    .project_combat(binding, &[CombatEvent::KillerNotification(text)], limits)
                    .map_err(|error| format!("death killer projection: {error:?}"))?;
                self.network_output.push_back(
                    crate::game_messages::session_batch_command(key, batch)
                        .map_err(|error| error.to_string())?,
                );
            }
            self.retain_observer_messages(vec![(
                actor,
                vec![ReplicationMessage {
                    queue: 10,
                    bytes: public,
                }],
            )])?;
            self.acknowledge_death_delivery(sequence)?;
        }
        Ok(())
    }

    fn project_death_started(&mut self, input: DeathStartProjection<'_>) -> Result<bool, String> {
        let DeathStartProjection {
            actor,
            accepted,
            num_deaths,
            death_level,
            vitae_pool,
            vitae,
            purge_bad,
            suicide,
        } = input;
        if accepted.entity != actor {
            return Err("death motion accepted actor mismatch".into());
        }
        let Some(motion) = accepted.motion.as_ref().ok().and_then(Option::as_ref) else {
            return Ok(false);
        };
        if !motion.actions.iter().any(|action| {
            action.domain == bace_gameplay_api::visibility::AcceptedMotionDomain::Death
        }) {
            return Ok(false);
        }
        let view = bace_replication::project_accepted_server_motion(accepted)
            .map_err(|error| format!("death accepted motion: {error:?}"))?;
        if self.network_output.len() >= self.limits.messages || !self.observer_room(1, 8192) {
            return Ok(false);
        }
        let replica = self
            .players
            .replication(actor)
            .ok_or("death motion canonical player missing")?;
        let key = replica.key;
        let binding = replica.binding;
        if self.sessions.get(&key).is_none_or(|s| s.disconnected) {
            // A disconnected generation has no private ordered publication;
            // retain the motion until its detach/visibility handoff resolves.
            return Ok(false);
        }
        let vitae_wire = if let Some(entry) = vitae {
            let table = self
                .sessions
                .get(&key)
                .and_then(|session| session.loading.as_ref())
                .and_then(|loading| loading.spell_table.as_ref())
                .ok_or("death vitae spell table missing")?;
            let definition = crate::player_assets::player_enchantment_definition(
                entry.spell,
                table,
                &self.assets.spell_rows,
            );
            let projection =
                crate::enchantment_saves::prepare_enchantment_projection(entry, definition)
                    .map_err(|error| format!("death vitae projection: {error}"))?;
            bace_replication::project_enchantments(&[projection], 1)
                .map_err(|error| format!("death vitae wire: {error:?}"))?
                .vitae
        } else {
            None
        };
        if vitae.is_some()
            && (death_level.is_none() || vitae_pool.is_none() || vitae_wire.is_none())
        {
            return Err("death vitae output evidence incomplete".into());
        }
        if vitae.is_none() && (death_level.is_some() || vitae_pool.is_some()) {
            return Err("death penalty fields lack vitae output".into());
        }
        let message_limit = self.limits.message_bytes;
        let motion_limits = BatchLimits {
            max_messages: 1,
            max_bytes: message_limit,
            max_message_bytes: message_limit,
            max_string_bytes: 4096,
        };
        // The death action is frozen before the save. Check its codec and
        // motion counter on a copy before advancing the private event owner.
        bace_replication::project_server_motion(
            actor.0,
            &view,
            &mut replica.properties.proposal_copy(),
            motion_limits,
        )
        .map_err(|error| format!("death motion preflight: {error:?}"))?;
        let mut steps = vec![
            P::Vital {
                vital: 2,
                current: 0,
            },
            P::PrivateProperty {
                property: 43,
                value: bace_wire::PropertyValue::Int(
                    i32::try_from(num_deaths).map_err(|_| "death count overflow")?,
                ),
            },
        ];
        if suicide {
            steps.push(P::Simple(bace_wire::SimpleGameEvent::WeenieError(0x4a)));
        }
        if let Some(level) = death_level {
            steps.push(P::PrivateProperty {
                property: 139,
                value: bace_wire::PropertyValue::Int(
                    i32::try_from(level).map_err(|_| "death level overflow")?,
                ),
            });
        }
        if let Some(pool) = vitae_pool {
            steps.push(P::PrivateProperty {
                property: 129,
                value: bace_wire::PropertyValue::Int(pool),
            });
        }
        if let Some(entry) = vitae_wire.as_ref() {
            steps.push(P::Magic(bace_wire::MagicEvent::UpdateEnchantment(entry)));
        }
        steps.push(P::Magic(if purge_bad {
            bace_wire::MagicEvent::PurgeBad
        } else {
            bace_wire::MagicEvent::Purge
        }));
        if purge_bad {
            steps.push(P::System {
                text: "Your augmentation prevents the tides of death from ripping away your current enchantments!",
                chat_type: 0,
            });
        }
        let private = replica
            .events
            .project_inventory_with_actor(
                binding,
                &steps,
                &mut replica.item_properties,
                Some(&mut replica.properties),
                bace_wire::ObjectCodecLimits {
                    max_message_bytes: message_limit,
                    max_model_entries: 255,
                    max_children: 128,
                    max_restrictions: 1024,
                    max_motion_commands: 32,
                    max_string_bytes: 4096,
                },
                BatchLimits {
                    max_messages: steps.len(),
                    max_bytes: message_limit.saturating_mul(steps.len()),
                    max_message_bytes: message_limit,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|error| format!("death start private projection: {error:?}"))?;
        let message = bace_replication::project_server_motion(
            actor.0,
            &view,
            &mut replica.properties,
            motion_limits,
        )
        .map_err(|error| format!("death motion canonical sequence: {error:?}"))?;
        let mut messages = Vec::with_capacity(1 + private.messages.len());
        messages.push(message.clone());
        messages.extend(private.messages);
        self.network_output.push_back(
            crate::game_messages::session_batch_command(key, SessionBatch { binding, messages })
                .map_err(|error| error.to_string())?,
        );
        self.retain_observer_messages(vec![(actor, vec![message])])?;
        Ok(true)
    }

    fn project_death_corpse(
        &mut self,
        operation: u64,
        corpse: EntityId,
        accepted: &bace_gameplay_api::visibility::AcceptedObjectView,
    ) -> Result<bool, String> {
        if accepted.entity != corpse || accepted.cell == 0 {
            return Err("death corpse accepted view mismatch".into());
        }
        let Some(source) = self
            .world
            .as_ref()
            .and_then(|world| world.regions.corpse_source(corpse))
        else {
            return Ok(false);
        };
        let saved = source.corpse.as_ref().ok_or("death corpse source absent")?;
        if saved.operation != Some(operation)
            || source.item.entity.object_id != corpse.0
            || source.item.persisted_version != 1
        {
            return Err("death corpse source receipt mismatch".into());
        }
        let bace_storage_codec::ItemPlacementV2::World(position) = &saved.placement else {
            return Err("death corpse visible placement mismatch".into());
        };
        if position.obj_cell_id != accepted.cell
            || [
                position.position_x,
                position.position_y,
                position.position_z,
            ] != accepted.position
        {
            return Err("death corpse accepted pose mismatch".into());
        }
        let Some(blueprint) = self
            .deaths
            .completed_presentations
            .get(&operation)
            .filter(|record| record.corpse == corpse)
            .and_then(|record| record.corpse_visibility.clone())
        else {
            return Ok(false);
        };
        if blueprint.description.object_id != corpse.0
            || blueprint.incarnation != operation
            || blueprint.revision != 1
        {
            return Err("death corpse blueprint identity mismatch".into());
        }
        let tick = u64::try_from(self.last_elapsed.as_nanos().saturating_mul(30) / 1_000_000_000)
            .map_err(|_| "death corpse visibility tick overflow")?;
        blueprint
            .register(&mut self.visibility.service, tick)
            .map_err(|error| format!("death corpse visibility registration: {error:?}"))?;
        self.deaths
            .completed_presentations
            .get_mut(&operation)
            .expect("validated death presentation")
            .corpse_visibility = None;
        Ok(true)
    }

    fn project_death_world_drops(
        &mut self,
        operation: u64,
        actor: EntityId,
        roots: &[(
            EntityId,
            Result<
                bace_gameplay_api::visibility::AcceptedObjectView,
                bace_gameplay_api::visibility::ObjectViewRejection,
            >,
        )],
    ) -> Result<bool, String> {
        let Some(record) = self
            .deaths
            .completed_presentations
            .get(&operation)
            .filter(|record| record.actor == actor && record.corpse.0 == 0)
        else {
            return Ok(false);
        };
        if roots.len() != record.world_visibility.len() || roots.len() > 1024 {
            return Err("NoCorpse world visibility count mismatch".into());
        }
        let blueprints = record.world_visibility.clone();
        for ((id, accepted), blueprint) in roots.iter().zip(&blueprints) {
            let Ok(accepted) = accepted else {
                return Ok(false);
            };
            let Some(source) = self
                .world
                .as_ref()
                .and_then(|world| world.regions.world_item_source(*id))
            else {
                return Ok(false);
            };
            let Some(bace_storage_codec::ItemPlacementV2::World(position)) = &source.item.placement
            else {
                return Err("NoCorpse world source placement invalid".into());
            };
            if accepted.entity != *id
                || accepted.cell != position.obj_cell_id
                || accepted.position
                    != [
                        position.position_x,
                        position.position_y,
                        position.position_z,
                    ]
                || blueprint.description.object_id != id.0
                || blueprint.incarnation != operation
                || blueprint.revision != source.item.persisted_version as u64
            {
                return Err("NoCorpse world visibility receipt mismatch".into());
            }
        }
        let tick = u64::try_from(self.last_elapsed.as_nanos().saturating_mul(30) / 1_000_000_000)
            .map_err(|_| "NoCorpse visibility tick overflow")?;
        for blueprint in &blueprints {
            blueprint
                .register(&mut self.visibility.service, tick)
                .map_err(|error| format!("NoCorpse visibility registration: {error:?}"))?;
        }
        self.deaths
            .completed_presentations
            .get_mut(&operation)
            .expect("validated NoCorpse presentation")
            .world_visibility
            .clear();
        Ok(true)
    }

    fn project_death_respawned(
        &mut self,
        operation: u64,
        actor: EntityId,
        accepted: &bace_gameplay_api::visibility::AcceptedObjectView,
        vitals: [u32; 3],
        revision: u64,
    ) -> Result<bool, String> {
        if accepted.entity != actor || accepted.epoch == 0 || revision == 0 {
            return Err("death respawn accepted view/revision mismatch".into());
        }
        if !self.death_portal_materialized(actor, operation, accepted.epoch) {
            return Ok(false);
        }
        let Some(binding) = self
            .deaths
            .completed_presentations
            .get(&operation)
            .filter(|record| {
                record.actor == actor
                    && record.corpse_visibility.is_none()
                    && record.world_visibility.is_empty()
            })
            .map(|record| record.binding)
        else {
            return Ok(false);
        };
        let Some(replica) = self.players.replication(actor) else {
            return Ok(true);
        };
        if replica.binding != binding {
            // A later login generation cannot receive an old death packet.
            return Ok(true);
        }
        let key = replica.key;
        if self
            .sessions
            .get(&key)
            .is_none_or(|session| session.disconnected)
        {
            // The portal owner's exact detach receipt owns this generation;
            // there is no private peer to send the vital batch to.
            return Ok(true);
        }
        let mut steps = Vec::with_capacity(3);
        for (index, vital) in [2, 4, 6].into_iter().enumerate() {
            if replica.vital_revisions[index].is_none_or(|seen| seen < revision) {
                steps.push(P::Vital {
                    vital,
                    current: vitals[index],
                });
            }
        }
        if steps.is_empty() {
            return Ok(true);
        }
        if self.network_output.len() >= self.limits.messages {
            return Ok(false);
        }
        let batch = replica
            .events
            .project_inventory_with_actor(
                replica.binding,
                &steps,
                &mut replica.item_properties,
                Some(&mut replica.properties),
                bace_wire::ObjectCodecLimits {
                    max_message_bytes: self.limits.message_bytes,
                    max_model_entries: 255,
                    max_children: 128,
                    max_restrictions: 1024,
                    max_motion_commands: 32,
                    max_string_bytes: 4096,
                },
                BatchLimits {
                    max_messages: 3,
                    max_bytes: self.limits.message_bytes.saturating_mul(3),
                    max_message_bytes: self.limits.message_bytes,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|error| format!("death respawn vital projection: {error:?}"))?;
        for (index, _) in [2, 4, 6].into_iter().enumerate() {
            if replica.vital_revisions[index].is_none_or(|seen| seen < revision) {
                replica.vital_revisions[index] = Some(revision);
            }
        }
        self.network_output.push_back(
            crate::game_messages::session_batch_command(key, batch)
                .map_err(|error| error.to_string())?,
        );
        Ok(true)
    }

    fn project_corpse_expired(
        &mut self,
        event: bace_simulation::CorpseExpiryEvent,
        ticket: &bace_simulation::CorpseExpiryTicket,
        spill: Option<&CorpseSpillPresentation>,
    ) -> Result<bool, String> {
        if event.corpse != ticket.corpse || event.death_operation != ticket.death_operation {
            return Err("corpse expiry event/ticket mismatch".into());
        }
        let tick = u64::try_from(self.last_elapsed.as_nanos().saturating_mul(30) / 1_000_000_000)
            .map_err(|_| "corpse expiry visibility tick overflow")?;
        match event.phase {
            bace_simulation::CorpseExpiryPhase::Destroying => {
                // ACE WorldObject_Decay broadcasts PlayScript.Destroy before its
                // one-second delayed removal. Spill roots are independently
                // visible only after the same placement receipt is committed.
                let script = CombatEffect::Script {
                    object_id: ticket.corpse.0,
                    script_id: 0x59,
                    speed: 1.0,
                }
                .encode(4096, self.limits.message_bytes)
                .map_err(|e| format!("corpse destroy script codec: {e:?}"))?;
                if !self.observer_room(1, script.len()) {
                    return Ok(false);
                }
                if let Some(presentation) = spill {
                    let sources = sources::committed_spill(ticket, presentation)?;
                    if presentation.visibility.len()
                        != ticket.spill.as_ref().map_or(0, |intent| intent.roots.len())
                    {
                        return Err("corpse spill visibility root count".into());
                    }
                    for (source, blueprint) in sources.iter().zip(&presentation.visibility) {
                        if blueprint.description.object_id != source.item.entity.object_id
                            || blueprint.revision != source.item.persisted_version as u64
                            || blueprint.incarnation != ticket.inventory.operation
                            || !matches!(
                                source.item.placement,
                                Some(bace_storage_codec::ItemPlacementV2::World(_))
                            )
                        {
                            return Err("corpse spill visibility receipt mismatch".into());
                        }
                    }
                    let block = ticket
                        .spill
                        .as_ref()
                        .ok_or("corpse spill intent absent")?
                        .position
                        .cell
                        >> 16;
                    let Some(world) = self.world.as_mut() else {
                        return Ok(false);
                    };
                    world
                        .regions
                        .record_committed_world_item_sources(block as u16, sources)?;
                    for blueprint in &presentation.visibility {
                        blueprint
                            .register(&mut self.visibility.service, tick)
                            .map_err(|e| format!("corpse spill visibility registration: {e:?}"))?;
                    }
                } else if ticket.spill.is_some() {
                    return Err("corpse spill committed presentation absent".into());
                }
                self.retain_observer_messages(vec![(
                    ticket.corpse,
                    vec![ReplicationMessage {
                        queue: 10,
                        bytes: script,
                    }],
                )])?;
            }
            bace_simulation::CorpseExpiryPhase::Removed => {
                self.visibility
                    .service
                    .retire_object(ticket.corpse, tick)
                    .map_err(|e| format!("corpse removal visibility: {e:?}"))?;
                let Some(world) = self.world.as_mut() else {
                    return Ok(false);
                };
                let mut ids = Vec::with_capacity(ticket.inventory.proposal.changes.len());
                for change in &ticket.inventory.proposal.changes {
                    if change.after.place == bace_inventory::ItemPlace::Removed {
                        ids.push(change.after.id);
                    }
                }
                world.regions.forget_inventory_sources(&ids)?;
            }
        }
        Ok(true)
    }
}
