//! Exact accepted portal views become retained private/observer packets. A cursor
//! commits each actor's projection once, even if another participant is blocked.
use super::*;
use bace_replication::InventoryProjection as P;
use bace_replication::{BatchLimits, PortalPhase, PortalView, SequenceKind};
impl GameRuntime {
    pub(super) fn project_portal_deliveries(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            let Some(delivery) = self.portals.deliveries.front() else {
                break;
            };
            // These transitions publish no portal packet. A failed spell has
            // already transferred its terminal result to magic; binding owns
            // its own exact receipt. Recall-origin failures still error below
            // and retain the ticket because ACE has no post-Teleport callback.
            let packetless = match &delivery.work {
                PortalDeliveryWork::Completed(_)
                | PortalDeliveryWork::Event(
                    PortalServiceEvent::Blocked { .. }
                    | PortalServiceEvent::AbortedAfterCommit { .. },
                ) => true,
                PortalDeliveryWork::Event(PortalServiceEvent::Linked { operation, .. }) => {
                    self.portals.tickets.get(operation).is_some_and(|ticket| {
                        ticket.origin == bace_simulation::PortalServiceOrigin::Binding
                    })
                }
                _ => false,
            };
            if !packetless
                && (self.visibility.service.pending()
                    || self.network_output.len() >= self.limits.messages)
            {
                break;
            }
            match &delivery.work {
                PortalDeliveryWork::Event(PortalServiceEvent::Linked { operation, actor }) => {
                    let (sequence, operation, actor) = (delivery.sequence, *operation, *actor);
                    self.project_portal_link(operation, actor)?;
                    self.portals.blocked_effects.remove(&operation);
                    self.portals.acknowledge(sequence)?;
                    continue;
                }
                PortalDeliveryWork::Event(PortalServiceEvent::Removed { entity }) => {
                    let (sequence, entity) = (delivery.sequence, *entity);
                    let tick = u64::try_from(
                        self.last_elapsed.as_nanos().saturating_mul(30) / 1_000_000_000,
                    )
                    .map_err(|_| "portal retirement tick overflow")?;
                    if self.visibility.retirements.len() >= 65536
                        && !self.visibility.retirements.contains_key(&entity)
                    {
                        break;
                    }
                    self.visibility.retirements.insert(entity, tick);
                    self.portals.acknowledge(sequence)?;
                    continue;
                }
                PortalDeliveryWork::Event(PortalServiceEvent::Blocked { operation, .. }) => {
                    // The committed transaction still owns the pending effect.
                    // A transient failure is observable through PortalService
                    // and cannot be reported as a final client failure.
                    let (sequence, operation) = (delivery.sequence, *operation);
                    if self.portals.blocked_effects.len() >= 4096
                        && !self.portals.blocked_effects.contains(&operation)
                    {
                        break;
                    }
                    self.portals.blocked_effects.insert(operation);
                    self.portals.acknowledge(sequence)?;
                    continue;
                }
                PortalDeliveryWork::Event(PortalServiceEvent::AbortedAfterCommit {
                    operation,
                    ..
                }) => {
                    let (sequence, operation) = (delivery.sequence, *operation);
                    if self.portals.tickets.get(&operation).is_some_and(|ticket| {
                        matches!(
                            ticket.origin,
                            bace_simulation::PortalServiceOrigin::Recall(_)
                        )
                    }) {
                        return Err(
                            "recall portal abort has no source-qualified failure output; retained"
                                .into(),
                        );
                    }
                    self.record_binding_portal_completion(operation, false)?;
                    self.portals.blocked_effects.remove(&operation);
                    self.portals.acknowledge(sequence)?;
                    continue;
                }
                PortalDeliveryWork::Completed(completion) => {
                    let (sequence, operation, committed, aborted) = (
                        delivery.sequence,
                        completion.work.ticket.operation,
                        completion.committed,
                        completion.aborted,
                    );
                    if self.portals.tickets.get(&operation) != Some(&completion.work.ticket) {
                        return Err("portal completion ticket identity mismatch; retained".into());
                    }
                    if (!committed || aborted)
                        && matches!(
                            completion.work.ticket.origin,
                            bace_simulation::PortalServiceOrigin::Recall(_)
                        )
                    {
                        return Err("recall portal failed completion has no source-qualified failure output; retained".into());
                    }
                    if !committed || aborted {
                        self.record_binding_portal_completion(operation, false)?;
                    }
                    self.portals.tickets.remove(&operation);
                    self.portals.blocked_effects.remove(&operation);
                    self.portals.acknowledge(sequence)?;
                    continue;
                }
                PortalDeliveryWork::Event(PortalServiceEvent::Summoned { .. }) => break,
                _ => {}
            }
            let operation = match &delivery.work {
                PortalDeliveryWork::Event(
                    PortalServiceEvent::Hidden { operation, .. }
                    | PortalServiceEvent::Teleported { operation, .. }
                    | PortalServiceEvent::Materialized { operation, .. },
                ) => *operation,
                _ => 0,
            };
            let (phase, view) = match &delivery.work {
                PortalDeliveryWork::Event(PortalServiceEvent::Hidden { actors, views, .. }) => {
                    validate_views(actors, views)?;
                    (PortalPhase::Hide, views.get(delivery.next_actor).copied())
                }
                PortalDeliveryWork::Event(PortalServiceEvent::Teleported {
                    actors, views, ..
                }) => {
                    validate_views(actors, views)?;
                    (
                        PortalPhase::Teleport,
                        views.get(delivery.next_actor).copied(),
                    )
                }
                PortalDeliveryWork::Event(PortalServiceEvent::Materialized {
                    actor, view, ..
                }) => {
                    if *actor != view.actor {
                        return Err("portal materialization identity mismatch".into());
                    }
                    (
                        PortalPhase::Materialize,
                        (delivery.next_actor == 0).then_some(*view),
                    )
                }
                // Other effects retain their explicit output obligation; they
                // cannot be consumed by a teleport-only projector.
                _ => break,
            };
            let Some(view) = view else {
                let sequence = delivery.sequence;
                if phase == PortalPhase::Teleport {
                    self.portals.blocked_effects.remove(&operation);
                }
                self.portals.acknowledge(sequence)?;
                continue;
            };
            if !self.observer_room(1, 4096) || !self.portals.publication_room() {
                break;
            }
            if phase == PortalPhase::Teleport
                && self
                    .portals
                    .transits
                    .get(&view.actor)
                    .is_some_and(|t| t.operation != operation || t.view != view)
            {
                return Err("portal transit output identity mismatch".into());
            }
            if phase == PortalPhase::Teleport
                && !self.portals.transits.contains_key(&view.actor)
                && self.portals.transits.len() >= 4096
            {
                break;
            }
            let r = self
                .players
                .replication(view.actor)
                .ok_or("portal output actor has no canonical session")?;
            let key = r.key;
            let binding = r.binding;
            if phase == PortalPhase::Materialize
                && operation == 0
                && self.portals.initial_ready.get(&key) != Some(&binding)
            {
                // Simulation and adapter receipts travel on separate bounded
                // channels. Keep the accepted view until the matching first
                // LoginComplete has crossed the authenticated control owner.
                break;
            }
            let before = r
                .public_physics_state
                .ok_or("portal initial public state missing")?;
            if phase == PortalPhase::Teleport
                && r.properties
                    .current(SequenceKind::ObjectTeleport, 0)
                    .wrapping_add(1)
                    != view.epoch
            {
                return Err("portal accepted epoch/counter mismatch".into());
            }
            let after = portal_state(before, phase, view.cloaked);
            let physics_state = match phase {
                PortalPhase::Hide => None,
                PortalPhase::Teleport => (before != after).then_some(after),
                PortalPhase::Materialize => Some(after),
            };
            let p = view.position;
            let position = bace_wire::PositionPack {
                position: bace_wire::WirePosition {
                    cell: p.cell,
                    origin: p.origin,
                    rotation: p.rotation,
                },
                velocity: Some(view.velocity),
                placement: None,
                grounded: view.grounded,
                instance_sequence: 0,
                position_sequence: 0,
                teleport_sequence: 0,
                force_position_sequence: 0,
            };
            let mut batch = bace_replication::project_portal(
                r.binding,
                phase,
                PortalView {
                    object: view.actor.0,
                    position,
                    physics_state,
                },
                &mut r.properties,
                BatchLimits {
                    max_messages: 4,
                    max_bytes: self.limits.message_bytes,
                    max_message_bytes: self.limits.message_bytes,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|e| format!("portal projection: {e:?}"))?;
            if phase == PortalPhase::Materialize
                && operation == 0
                && self.portals.initial_materialized.get(&key) != Some(&binding)
                && !self.assets.policy.require_spell_components
            {
                // Pinned ACE guards this first-entry override with
                // FirstEnterWorldDone in GameActionLoginComplete. Keep it in
                // the materialization's reliable owner batch once per session.
                let override_batch = r
                    .events
                    .project_inventory_with_actor(
                        r.binding,
                        &[P::PrivateProperty {
                            property: 68,
                            value: bace_wire::PropertyValue::Bool(false),
                        }],
                        &mut r.item_properties,
                        Some(&mut r.properties),
                        bace_wire::ObjectCodecLimits {
                            max_message_bytes: self.limits.message_bytes,
                            max_model_entries: 255,
                            max_children: 128,
                            max_restrictions: 1024,
                            max_motion_commands: 32,
                            max_string_bytes: 4096,
                        },
                        BatchLimits {
                            max_messages: 1,
                            max_bytes: self.limits.message_bytes,
                            max_message_bytes: self.limits.message_bytes,
                            max_string_bytes: 4096,
                        },
                    )
                    .map_err(|e| format!("initial portal override: {e:?}"))?;
                batch.owner.messages.extend(override_batch.messages);
            }
            let reset = batch.reset_visibility;
            r.public_physics_state = Some(after);
            let output =
                crate::portal_output::portal_output(key, batch).map_err(|e| e.to_string())?;
            let NetworkCommand::SendOrderedBatch { key, messages } = output.owner else {
                return Err("portal owner projection command mismatch".into());
            };
            let command = self.portals.publications.retain(
                view.actor,
                key,
                messages,
                (phase == PortalPhase::Materialize).then_some((operation, view.epoch)),
            )?;
            self.network_output.push_back(command);
            self.retain_observer_messages(vec![(view.actor, output.observers)])?;
            if phase == PortalPhase::Materialize && operation == 0 {
                self.portals.initial_materialized.insert(key, binding);
            }
            if reset {
                self.mark_last_observer_visibility_reset();
            }
            if phase == PortalPhase::Teleport {
                self.portals
                    .transits
                    .entry(view.actor)
                    .or_insert(controls::Transit {
                        operation,
                        view,
                        destination_ready: false,
                        materialized: false,
                    });
            } else if phase == PortalPhase::Materialize
                && let Some(t) = self.portals.transits.get_mut(&view.actor)
            {
                t.materialized = true;
            }
            self.portals
                .deliveries
                .front_mut()
                .expect("retained portal delivery")
                .next_actor += 1;
        }
        Ok(())
    }
    fn project_portal_link(&mut self, operation: u64, actor: EntityId) -> Result<(), String> {
        let ticket = self
            .portals
            .tickets
            .get(&operation)
            .ok_or("portal link ticket missing")?
            .clone();
        if ticket.actor != actor {
            return Err("portal link actor mismatch".into());
        }
        if ticket.origin == bace_simulation::PortalServiceOrigin::Binding {
            return self.record_binding_portal_completion(operation, true);
        }
        let bace_simulation::PortalServiceEffect::Link(change) = ticket.effect else {
            return Err("portal link effect mismatch".into());
        };
        let text = if change.position_slot == 4 {
            "You have successfully linked with the life stone."
        } else {
            "You have successfully linked with the portal."
        };
        if self.network_output.len() >= self.limits.messages {
            return Err("portal link output pressure retained".into());
        }
        let r = self
            .players
            .replication(actor)
            .ok_or("portal link recipient missing")?;
        let batch = r
            .events
            .project_inventory_with_actor(
                r.binding,
                &[P::System { text, chat_type: 7 }],
                &mut r.item_properties,
                Some(&mut r.properties),
                bace_wire::ObjectCodecLimits {
                    max_message_bytes: self.limits.message_bytes,
                    max_model_entries: 255,
                    max_children: 128,
                    max_restrictions: 1024,
                    max_motion_commands: 32,
                    max_string_bytes: 4096,
                },
                BatchLimits {
                    max_messages: 2,
                    max_bytes: self.limits.message_bytes,
                    max_message_bytes: self.limits.message_bytes,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|error| format!("portal link projection: {error:?}"))?;
        self.network_output.push_back(
            crate::game_messages::session_batch_command(r.key, batch).map_err(|e| e.to_string())?,
        );
        Ok(())
    }
}
fn validate_views(
    actors: &[EntityId],
    views: &[bace_simulation::PortalAcceptedView],
) -> Result<(), String> {
    if actors
        .iter()
        .enumerate()
        .any(|(i, actor)| actor.0 == 0 || actors[..i].contains(actor))
        || actors.is_empty()
        || actors.len() > 9
        || actors.len() != views.len()
        || actors
            .iter()
            .zip(views)
            .any(|(actor, view)| *actor != view.actor)
    {
        return Err("portal accepted view identity/capacity".into());
    }
    Ok(())
}
fn portal_state(before: u32, phase: PortalPhase, cloaked: bool) -> u32 {
    match phase {
        PortalPhase::Hide => before,
        PortalPhase::Teleport => (before | 0x4010) & !8,
        PortalPhase::Materialize => {
            let visible = before & !0x4010;
            if cloaked { visible } else { visible | 8 }
        }
    }
}

#[cfg(test)]
mod tests;
