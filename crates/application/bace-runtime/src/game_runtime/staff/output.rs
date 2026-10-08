use super::*;
use bace_replication::{BatchLimits, SessionBatch};
impl GameRuntime {
    pub(in crate::game_runtime::staff) fn poll_staff_output(&mut self) -> Result<(), String> {
        let limits = BatchLimits {
            max_messages: 4096,
            max_bytes: self.limits.message_bytes,
            max_message_bytes: self.limits.message_bytes,
            max_string_bytes: 4096,
        };
        for _ in 0..self.limits.work_per_poll {
            self.poll_staff_broadcast_log()?;
            if let Some(command) = self.staff.output.pop_front() {
                if let Err(error) = self.network.try_send(command) {
                    self.staff.output.push_front(match error {
                        std::sync::mpsc::TrySendError::Full(c)
                        | std::sync::mpsc::TrySendError::Disconnected(c) => c,
                    });
                    break;
                }
                continue;
            }
            if self.poll_staff_broadcast()? {
                continue;
            }
            if self.staff.event.is_none() {
                self.staff.event = self.simulation.staff_events().try_recv().ok();
            }
            let Some(event) = self.staff.event.clone() else {
                break;
            };
            if self.accept_player_magic_program_outcome(&event)?
                || self.accept_staff_native_outcome(&event)?
                || self.accept_staff_ban_audit_outcome(&event)?
            {
                self.staff.event = None;
                continue;
            }
            match &event {
                StaffEvent::TargetQuery(event) => {
                    let r = self
                        .players
                        .replication(event.context.actor)
                        .ok_or("query recipient missing")?;
                    let batch = r
                        .events
                        .project_target_query(r.binding, *event, limits)
                        .map_err(|e| format!("query projection: {e:?}"))?;
                    if !batch.messages.is_empty() {
                        self.staff.output.push_back(
                            crate::game_messages::session_batch_command(r.key, batch)
                                .map_err(|e| e.to_string())?,
                        );
                    }
                }

                StaffEvent::GagProposal(ticket) => {
                    if self.staff.gag.is_some()
                        || self.staff.pending.as_ref().is_none_or(|p| {
                            p.token != ticket.operation || p.context != ticket.context
                        })
                    {
                        return Err("gag proposal correlation".into());
                    }
                    self.staff.gag = Some(gag::GagPending::new(ticket.clone()));
                }
                StaffEvent::Broadcast { .. } => {
                    if !self.retain_staff_broadcast(&event)? {
                        break;
                    }
                }
                StaffEvent::Outcome {
                    token,
                    actor,
                    result,
                } => {
                    let pending = self
                        .staff
                        .pending
                        .as_ref()
                        .ok_or("staff outcome without retained request")?;
                    if *token != pending.token
                        || *actor != Some(pending.context.actor)
                        || !matches!(pending.phase, Phase::Awaiting)
                    {
                        return Err("staff outcome correlation".into());
                    }
                    let gag_receipt = self.staff.gag.is_some();
                    if !self.finish_staff_spell(*token, *result)?
                        || !self.finish_staff_gag(*token, *result)?
                    {
                        self.staff.event = None;
                        return Err(self
                            .staff
                            .failure
                            .clone()
                            .expect("retained staff receipt failure"));
                    }
                    self.staff.pending = None;
                    if let Err(error) = result
                        && !gag_receipt
                    {
                        self.staff.failure = Some(format!("staff action rejected: {error:?}"));
                    }
                }
                StaffEvent::SpellProposal(ticket) => {
                    if self.staff.spell.is_some()
                        || self.staff.pending.as_ref().is_none_or(|p| {
                            p.token != ticket.operation || p.context != ticket.context
                        })
                    {
                        return Err("staff spell proposal correlation".into());
                    }
                    self.staff.spell = Some(spell::SpellPending::new(ticket.clone()));
                }
                StaffEvent::Inspection { context, .. } => {
                    let r = self
                        .players
                        .replication(context.actor)
                        .ok_or("staff text recipient missing")?;
                    let batch = bace_replication::project_staff_text(r.binding, &event, limits)
                        .map_err(|e| format!("staff text projection: {e:?}"))?;
                    if !batch.messages.is_empty() {
                        self.staff.output.push_back(
                            crate::game_messages::session_batch_command(r.key, batch)
                                .map_err(|e| e.to_string())?,
                        );
                    }
                }
                StaffEvent::Healed { target, .. } => {
                    let r = self
                        .players
                        .replication(*target)
                        .ok_or("staff heal recipient missing")?;
                    let batch = bace_replication::project_staff_heal(
                        r.binding,
                        &event,
                        &mut r.properties,
                        limits,
                    )
                    .map_err(|e| format!("staff heal projection: {e:?}"))?;
                    self.staff.output.push_back(
                        crate::game_messages::session_batch_command(r.key, batch)
                            .map_err(|e| e.to_string())?,
                    );
                }
                StaffEvent::Spellbook { context, .. } => {
                    let r = self
                        .players
                        .replication(context.actor)
                        .ok_or("staff spell recipient missing")?;
                    let batch = r
                        .events
                        .project_staff_spellbook(r.binding, &event, limits)
                        .map_err(|e| format!("staff spellbook projection: {e:?}"))?;
                    self.staff.output.push_back(
                        crate::game_messages::session_batch_command(r.key, batch)
                            .map_err(|e| e.to_string())?,
                    );
                }
                StaffEvent::Scripts { targets, .. } => {
                    let messages =
                        bace_replication::staff_magic::project_staff_scripts(&event, limits)
                            .map_err(|e| format!("staff script projection: {e:?}"))?;
                    let mut grouped = BTreeMap::new();
                    for ((target, _), message) in targets.iter().zip(messages) {
                        grouped
                            .entry(*target)
                            .or_insert_with(Vec::new)
                            .push(message);
                    }
                    let batches: Vec<_> = grouped.into_iter().collect();
                    let mut owners = vec![];
                    for (target, messages) in &batches {
                        if let Some(r) = self.players.replication(*target) {
                            owners.push(
                                crate::game_messages::session_batch_command(
                                    r.key,
                                    SessionBatch {
                                        binding: r.binding,
                                        messages: messages.clone(),
                                    },
                                )
                                .map_err(|e| e.to_string())?,
                            );
                        }
                    }
                    self.retain_observer_messages(batches)?;
                    self.staff.output.extend(owners);
                }
                StaffEvent::Teleported {
                    target,
                    after,
                    epoch,
                    velocity,
                    grounded,
                    ..
                } => {
                    // Check observer capacity before advancing canonical stamps.
                    if !self.observer_room(1, 4096) {
                        return Err("staff teleport waits for observer routing capacity".into());
                    }
                    let r = self
                        .players
                        .replication(*target)
                        .ok_or("staff teleport recipient missing")?;
                    if r.properties
                        .current(bace_replication::SequenceKind::ObjectTeleport, 0)
                        .wrapping_add(1)
                        != *epoch
                    {
                        return Err("staff teleport accepted epoch/counter mismatch".into());
                    }
                    let position = bace_wire::PositionPack {
                        position: bace_wire::WirePosition {
                            cell: after.cell,
                            origin: after.origin,
                            rotation: after.rotation,
                        },
                        velocity: Some(*velocity),
                        placement: None,
                        grounded: *grounded,
                        instance_sequence: 0,
                        position_sequence: 0,
                        teleport_sequence: *epoch,
                        force_position_sequence: 0,
                    };
                    let batch = bace_replication::project_staff_teleport(
                        r.binding,
                        &event,
                        position,
                        None,
                        &mut r.properties,
                        limits,
                    )
                    .map_err(|e| format!("staff teleport projection: {e:?}"))?;
                    let owner = crate::game_messages::session_batch_command(r.key, batch.owner)
                        .map_err(|e| e.to_string())?;
                    self.retain_observer_messages(vec![(*target, batch.observers)])?;
                    self.mark_last_observer_visibility_reset();
                    self.staff.output.push_back(owner);
                }
            }
            self.staff.event = None;
        }
        Ok(())
    }
}
