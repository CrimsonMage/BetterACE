//! Authenticated item hand-in. Item removal and the unexecuted Give continuation
//! commit together; canonical inventory output follows the real owner receipt.
use super::*;
use crate::npc_persistence::{
    NpcHandInResolution, NpcHandInStageInput, PendingNpcHandIn, freeze_handin_stage,
};
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_simulation::{
    NpcHandInRequest, NpcHandInTicket, PlayerReadSnapshot, PlayerSnapshotOperation,
    PlayerSnapshotRequest,
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    Ready,
    Saving,
    Delivering,
    Output,
}
pub(super) struct Use {
    pub binding: CharacterBinding,
    pub key: SessionKey,
    pub result: Option<u32>,
}
pub(super) struct HandIn {
    request: NpcHandInRequest,
    binding: CharacterBinding,
    key: SessionKey,
    phase: Phase,
    ticket: Option<NpcHandInTicket>,
    pub capture: Option<u64>,
    pub snapshot: Option<(Arc<PlayerReadSnapshot>, u64)>,
    pending: Option<PendingNpcHandIn>,
    critical: bool,
    committed: bool,
    rows: Vec<bace_persistence::SaveSnapshot>,
}
pub(super) fn accept(
    npc: &mut NpcRuntime,
    online: &mut crate::online_player_saves::OnlinePlayerSaveService,
    event: NpcCoordinatorEvent,
) -> Result<(), (String, NpcCoordinatorEvent)> {
    let source = match &event {
        NpcCoordinatorEvent::Command { source, .. }
        | NpcCoordinatorEvent::HandIn { source, .. }
        | NpcCoordinatorEvent::Held { source, .. } => *source,
        _ => return Err(("NPC hand-in event mismatch".into(), event)),
    };
    let Some(work) = npc.handins.get_mut(&source) else {
        return Err(("NPC hand-in owner missing".into(), event));
    };
    match &event {
        NpcCoordinatorEvent::Command { outcome, .. } => match &outcome.result {
            Ok(R::HandIn(ticket)) if work.phase == Phase::Preparing => {
                if ticket.request != work.request {
                    return Err(("NPC hand-in request mismatch".into(), event));
                }
                work.ticket = Some(ticket.clone());
                work.phase = Phase::Ready;
            }
            Ok(R::Applied) if work.phase == Phase::Delivering => {
                if work.critical {
                    let result = if work.committed {
                        online.finish_critical_for(&[work.binding.actor.0], &work.rows)
                    } else {
                        online.cancel_critical(&[work.binding.actor.0])
                    };
                    if let Err(error) = result {
                        return Err((error, event));
                    }
                    work.critical = false;
                }
                work.phase = Phase::Output;
            }
            Err(error) if work.phase == Phase::Preparing => {
                npc.failure = Some(format!("NPC hand-in rejected: {error:?}"));
                work.phase = Phase::Output;
            }
            Err(error) => npc.failure = Some(format!("NPC hand-in adoption held: {error:?}")),
            _ => return Err(("NPC hand-in response mismatch".into(), event)),
        },
        NpcCoordinatorEvent::HandIn { resolution, .. } => match resolution.as_ref() {
            NpcHandInResolution::Committed { ticket, .. } => {
                if work.ticket.as_ref() != Some(ticket) {
                    return Err(("NPC hand-in durable ticket mismatch".into(), event));
                }
                work.committed = true;
                work.phase = Phase::Delivering;
            }
            NpcHandInResolution::Rejected { .. } => {
                work.phase = Phase::Delivering;
            }
            NpcHandInResolution::Uncertain(error) => npc.failure = Some(error.clone()),
        },
        NpcCoordinatorEvent::Held { message, .. } => npc.failure = Some(message.clone()),
        _ => return Err(("NPC hand-in event mismatch".into(), event)),
    }
    Ok(())
}
impl GameRuntime {
    pub(in crate::game_runtime) fn npc_session_pending(&self, key: SessionKey) -> bool {
        self.npc.uses.values().any(|u| u.key == key)
            || self.npc.handins.values().any(|h| h.key == key)
            || self
                .sessions
                .get(&key)
                .and_then(|s| s.loading.as_ref())
                .is_some_and(|l| {
                    self.npc
                        .work
                        .values()
                        .any(|w| w.involves(l.loaded.binding.actor))
                        || self
                            .npc
                            .inventory_output
                            .iter()
                            .any(|d| d.binding.actor == l.loaded.binding.actor)
                })
    }
    pub(in crate::game_runtime) fn handle_npc_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<super::super::inventory::InventoryIngress, String> {
        use super::super::inventory::InventoryIngress as I;
        use bace_wire::{
            GameActionEnvelope, InventoryAction,
            opcode::{GameActionType as Op, GameMessageOpcode},
        };
        if message
            .bytes
            .get(..4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
            != Some(GameMessageOpcode::GameAction.0)
        {
            return Ok(I::Unsupported);
        }
        let envelope = GameActionEnvelope::decode(&message.bytes, self.limits.message_bytes)
            .map_err(|e| e.to_string())?;
        if !matches!(envelope.action, Op::GiveObjectRequest | Op::Use) {
            return Ok(I::Unsupported);
        }
        let decoded = bace_wire::InventoryRequest::decode(
            envelope.action,
            envelope.payload,
            self.limits.message_bytes,
            0,
        )
        .map_err(|e| e.to_string())?;
        let source = match decoded.action {
            InventoryAction::Give { target_id, .. } | InventoryAction::Use(target_id) => {
                EntityId(target_id)
            }
            _ => return Ok(I::Unsupported),
        };
        if self.npc.coordinator.binding(source).is_none() {
            return Ok(I::Unsupported);
        }
        let Some(session) = self.sessions.get(&key) else {
            return Ok(I::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(I::Blocked);
        };
        let binding = loading.loaded.binding;
        if self.draining
            || !self.players.entered(binding.actor)
            || session.disconnected
            || session.terminated
            || self.npc_session_pending(key)
            || self.npc.coordinator.busy(source)
            || self.npc.work.contains_key(&source)
            || self.npc.handins.len() == 64
        {
            return Ok(I::Blocked);
        }
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("NPC authenticated session binding".into());
        }
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: envelope.sequence,
        };
        let mut event = [0; 16];
        OsRng.fill_bytes(&mut event);
        let operation = self.token()?;
        if self
            .npc
            .coordinator
            .begin_invocation(source, event)
            .is_err()
        {
            return Ok(I::Blocked);
        }
        match decoded.action {
            InventoryAction::Give {
                item_id, amount, ..
            } => {
                let count = u32::try_from(amount)
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or("invalid NPC hand-in count")?;
                let request = NpcHandInRequest {
                    context,
                    source,
                    item: EntityId(item_id),
                    count,
                    event,
                    operation,
                };
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::PrepareHandIn {
                            request: request.clone(),
                        },
                    )
                    .map_err(|_| "NPC hand-in admission pressure")?;
                self.npc.handins.insert(
                    source,
                    HandIn {
                        request,
                        binding,
                        key,
                        phase: Phase::Preparing,
                        ticket: None,
                        capture: None,
                        snapshot: None,
                        pending: None,
                        critical: false,
                        committed: false,
                        rows: vec![],
                    },
                );
            }
            InventoryAction::Use(_) => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::Use {
                            context,
                            source,
                            event,
                            operation,
                        },
                    )
                    .map_err(|_| "NPC use admission pressure")?;
                self.npc.uses.insert(
                    source,
                    Use {
                        binding,
                        key,
                        result: None,
                    },
                );
            }
            _ => return Ok(I::Unsupported),
        }
        Ok(I::Accepted)
    }
    pub(super) fn poll_npc_handins(&mut self) -> Result<(), String> {
        let ids: Vec<_> = self.npc.handins.keys().copied().collect();
        for source in ids {
            if self.npc.coordinator.busy(source) {
                continue;
            }
            let correlation = self.token()?;
            let work = self.npc.handins.get_mut(&source).expect("retained hand-in");
            if work.phase == Phase::Ready {
                let ticket = work.ticket.as_ref().ok_or("NPC hand-in ticket missing")?;
                let actor = work.binding.actor.0;
                if !work.critical {
                    if !self.online_saves.critical_ready(&[actor])? {
                        continue;
                    }
                    self.online_saves.begin_critical(&[actor])?;
                    work.critical = true;
                }
                if work.snapshot.is_none() {
                    if work.capture.is_none()
                        && self
                            .simulation
                            .input()
                            .try_submit(bace_simulation::Command::PlayerSnapshot(
                                PlayerSnapshotRequest {
                                    correlation,
                                    binding: work.binding,
                                    operation: Some((
                                        PlayerSnapshotOperation::NpcHandIn {
                                            operation: ticket.inventory.operation,
                                        },
                                        ticket.character_revision,
                                    )),
                                },
                            ))
                            .is_ok()
                    {
                        work.capture = Some(correlation);
                    }
                    continue;
                }
                let (snapshot, unix) = work.snapshot.as_ref().expect("captured hand-in");
                let (baseline, version, lease) = self
                    .online_saves
                    .baseline(actor)
                    .ok_or("NPC hand-in baseline missing")?;
                let player = crate::player_saves::freeze_player_operation_baseline(
                    baseline,
                    snapshot,
                    PlayerSnapshotOperation::NpcHandIn {
                        operation: ticket.inventory.operation,
                    },
                    ticket.character_revision,
                    *unix,
                )
                .map_err(|e| e.to_string())?;
                let mut others = self.online_saves.operation_inventory_changes(snapshot)?;
                others.retain(|s| {
                    !ticket
                        .inventory
                        .proposal
                        .changes
                        .iter()
                        .any(|c| c.after.id.0 == s.object_id)
                });
                others.push(bace_persistence::SaveSnapshot {
                    object_id: actor,
                    mutation_revision: player.player.entity.mutation_revision,
                    expected_version: version,
                    bytes: player.encode().map_err(|e| e.to_string())?,
                });
                let items = self.online_saves.operation_inventory_baselines(snapshot)?;
                let positions = BTreeMap::new();
                let (binding, _) = self
                    .npc
                    .coordinator
                    .binding(source)
                    .ok_or("NPC hand-in source binding missing")?;
                let mut pending = freeze_handin_stage(NpcHandInStageInput {
                    binding,
                    world_epoch: self.bootstrap.world_owner.epoch(),
                    ticket,
                    inventory: crate::game_inventory::InventoryFreezeInput {
                        operation_id: "npc-hand-in-replaced",
                        proposal: &ticket.inventory.proposal,
                        items: &items,
                        other_snapshots: &others,
                        leases: &[lease],
                        storage_views: &[],
                        admitted_positions: &positions,
                    },
                })
                .map_err(|e| e.to_string())?;
                work.rows = pending.operation().inventory.snapshots.clone();
                if let Some(frozen) = super::source_inventory::prepare(
                    &mut self.npc.source_inventory,
                    self.world.as_ref().map(|w| &w.regions),
                    &self.npc.definitions,
                    ticket.checkpoint.inventory.as_ref(),
                    self.bootstrap.world_owner.epoch(),
                )? {
                    pending
                        .attach_source_inventory(&frozen)
                        .map_err(|e| e.to_string())?;
                }
                for row in &mut work.rows {
                    row.expected_version += 1;
                }
                work.pending = Some(pending);
                work.phase = Phase::Saving;
            } else if work.phase == Phase::Saving
                && let Some(pending) = work.pending.take()
                && let Err(pending) = self.npc.coordinator.enqueue_handin(source, pending)
            {
                work.pending = Some(*pending);
            }
        }
        self.project_npc_uses()?;
        self.project_npc_handins()
    }
    fn project_npc_uses(&mut self) -> Result<(), String> {
        let ready: Vec<_> = self
            .npc
            .uses
            .iter()
            .filter_map(|(source, p)| p.result.map(|code| (*source, code)))
            .collect();
        for (source, code) in ready {
            if self.network_output.len() >= self.limits.messages {
                break;
            }
            let work = &self.npc.uses[&source];
            let replica = self
                .players
                .replication(work.binding.actor)
                .ok_or("NPC use sequence owner absent")?;
            if replica.binding != work.binding || replica.key != work.key {
                return Err("NPC use output binding mismatch".into());
            }
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
            let batch = replica
                .events
                .project_inventory(
                    work.binding,
                    &[bace_replication::InventoryProjection::Simple(
                        bace_wire::SimpleGameEvent::UseDone(code),
                    )],
                    &mut replica.item_properties,
                    objects,
                    limits,
                )
                .map_err(|e| format!("NPC UseDone: {e:?}"))?;
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch {
                    key: work.key,
                    messages: batch
                        .messages
                        .into_iter()
                        .map(|m| (m.queue, m.bytes))
                        .collect(),
                });
            self.npc.uses.remove(&source);
        }
        Ok(())
    }
    fn project_npc_handins(&mut self) -> Result<(), String> {
        use bace_replication::InventoryProjection as P;
        let ids: Vec<_> = self
            .npc
            .handins
            .iter()
            .filter_map(|(s, h)| (h.phase == Phase::Output).then_some(*s))
            .collect();
        for source in ids {
            if self.network_output.len() == self.limits.messages {
                break;
            }
            let work = &self.npc.handins[&source];
            let mut steps = Vec::new();
            let mut removed = Vec::new();
            if work.committed
                && work
                    .ticket
                    .as_ref()
                    .is_some_and(|ticket| ticket.accepted_count > 0)
            {
                for change in &work
                    .ticket
                    .as_ref()
                    .ok_or("committed hand-in ticket missing")?
                    .inventory
                    .proposal
                    .changes
                {
                    if change.after.place == bace_inventory::ItemPlace::Removed {
                        steps.push(P::Remove(change.after.id));
                        removed.push(change.after.id);
                    } else {
                        steps.push(P::Stack {
                            item: change.after.id,
                            quantity: change.after.stack,
                            value: change
                                .after
                                .stack
                                .checked_mul(change.after.unit_value)
                                .ok_or("NPC stack value overflow")?,
                        });
                    }
                }
            } else {
                steps.push(P::Event(bace_wire::InventoryEvent::SaveFailed {
                    item_id: work.request.item.0,
                    error: if work.committed { 0x04cd } else { 0 },
                }));
            }
            let replica = self
                .players
                .replication(work.binding.actor)
                .ok_or("NPC hand-in output owner missing")?;
            if replica.binding != work.binding || replica.key != work.key {
                return Err("NPC hand-in output binding mismatch".into());
            }
            let limits = bace_replication::BatchLimits {
                max_messages: self.limits.messages,
                max_bytes: self.limits.message_bytes,
                max_message_bytes: self.limits.message_bytes,
                max_string_bytes: 4096,
            };
            let batch = replica
                .events
                .project_inventory(
                    work.binding,
                    &steps,
                    &mut replica.item_properties,
                    bace_wire::ObjectCodecLimits {
                        max_message_bytes: self.limits.message_bytes,
                        max_model_entries: 255,
                        max_children: 255,
                        max_restrictions: 1024,
                        max_motion_commands: 4096,
                        max_string_bytes: 4096,
                    },
                    limits,
                )
                .map_err(|e| format!("NPC hand-in projection: {e:?}"))?;
            for id in removed {
                replica.item_properties.remove(&id);
            }
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch {
                    key: work.key,
                    messages: batch
                        .messages
                        .into_iter()
                        .map(|m| (m.queue, m.bytes))
                        .collect(),
                });
            self.npc.handins.remove(&source);
        }
        Ok(())
    }
}
