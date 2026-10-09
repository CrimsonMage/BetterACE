//! Idle sources publish one final checkpoint before accepting a fresh invocation.
use super::*;
use crate::npc_persistence::{
    NpcCheckpointResolution, PendingNpcCheckpoint, freeze_terminal_checkpoint,
};
pub(super) struct Idle {
    pub operation: u64,
    checkpoint: Option<bace_simulation::NpcSourceCheckpoint>,
    pending: Option<PendingNpcCheckpoint>,
    releasing: bool,
    archived: bool,
    committed: bool,
    cleanup: bool,
    cleaning: bool,
}
#[cfg(test)]
impl NpcRuntime {
    pub(in crate::game_runtime) fn static_shop_diagnostic(&self, source: EntityId) -> String {
        let idle = self.idle.get(&source);
        let baseline = self
            .definitions
            .get(&source)
            .and_then(|r| r.baseline.as_ref());
        format!(
            "binding={:?} ready={} terminal={} busy={} idle={} checkpoint={} pending={} releasing={} committed={} source_frozen={} baseline={:?} failure={:?}",
            self.coordinator.binding(source),
            self.coordinator.source_ready(source),
            self.coordinator.needs_terminal(source),
            self.coordinator.busy(source),
            idle.is_some(),
            idle.is_some_and(|i| i.checkpoint.is_some()),
            idle.is_some_and(|i| i.pending.is_some()),
            idle.is_some_and(|i| i.releasing),
            idle.is_some_and(|i| i.committed),
            self.source_inventory.contains_key(&source),
            baseline.map(|b| (b.persisted_version, b.bytes.len())),
            self.failure.as_deref(),
        )
    }
}
pub(super) fn accept(
    npc: &mut NpcRuntime,
    event: NpcCoordinatorEvent,
) -> Result<(), (String, NpcCoordinatorEvent)> {
    let source = match &event {
        NpcCoordinatorEvent::Checkpoint { source, .. }
        | NpcCoordinatorEvent::Command { source, .. }
        | NpcCoordinatorEvent::Held { source, .. } => *source,
        _ => return Err(("NPC terminal event mismatch".into(), event)),
    };
    let Some(idle) = npc.idle.get_mut(&source) else {
        return Err(("NPC terminal owner missing".into(), event));
    };
    match &event {
        NpcCoordinatorEvent::Checkpoint { resolution, .. } => match resolution {
            NpcCheckpointResolution::Committed => {
                idle.committed = true;
                idle.releasing = true;
            }
            NpcCheckpointResolution::Rejected(_) => {
                idle.committed = false;
                idle.releasing = true;
            }
            NpcCheckpointResolution::Uncertain(error) => npc.failure = Some(error.clone()),
        },
        NpcCoordinatorEvent::Command { outcome, .. } => match &outcome.result {
            Ok(R::Checkpoint(checkpoint)) => {
                idle.archived = checkpoint.archive.is_some();
                idle.checkpoint = Some(checkpoint.clone());
            }
            Ok(R::Applied) if idle.cleaning => {
                if let Err(error) = npc.coordinator.release_source(source) {
                    return Err((error, event));
                }
                npc.idle.remove(&source);
                npc.definitions.remove(&source);
            }
            Ok(R::Applied) if idle.releasing => {
                if idle.archived && idle.committed {
                    idle.releasing = false;
                    idle.cleanup = true;
                } else {
                    npc.idle.remove(&source);
                }
            }
            Err(_) if !idle.releasing && !idle.cleaning => {
                npc.idle.remove(&source);
            }
            Err(error) => npc.failure = Some(format!("NPC terminal release: {error:?}")),
            _ => return Err(("NPC terminal receipt mismatch".into(), event)),
        },
        NpcCoordinatorEvent::Held { message, .. } => npc.failure = Some(message.clone()),
        _ => return Err(("NPC terminal event mismatch".into(), event)),
    }
    Ok(())
}
impl GameRuntime {
    pub(super) fn poll_npc_terminals(&mut self) -> Result<(), String> {
        // Entry and region handoff temporarily lend the region metadata owner
        // to another retained stage. Keep the exact NPC candidate/checkpoint
        // until it returns; freezing without its item source closure cannot be
        // committed or safely rolled back.
        if self.world.is_none() {
            return Ok(());
        }
        let candidates: Vec<_> =
            self.npc
                .coordinator
                .terminal_candidates()
                .filter(|s| {
                    !self.npc.handins.contains_key(s)
                        && !self.npc.work.contains_key(s)
                        && !self.npc.proposals.iter().any(|p| p.context.source == *s)
                        && !self.npc.idle.contains_key(s)
                        && self.npc.shared.as_ref().is_none_or(|(t, _)| {
                            t.npc.as_ref().is_none_or(|p| p.context.source != *s)
                        })
                })
                .collect();
        for source in candidates
            .into_iter()
            .cycle()
            .skip(self.npc.terminal_cursor)
            .take(self.limits.work_per_poll)
        {
            if self.npc.idle.contains_key(&source) {
                break;
            }
            let operation = self.token()?;
            let (binding, workflow_version) = self
                .npc
                .coordinator
                .binding(source)
                .ok_or("NPC terminal source binding")?;
            let bootstrap_shop = binding.source_version == 0
                && workflow_version == 0
                && self
                    .npc
                    .definitions
                    .get(&source)
                    .is_some_and(|registration| registration.baseline.is_none())
                && crate::npc_persistence::is_authored_static_shop(
                    self.npc
                        .definitions
                        .get(&source)
                        .ok_or("NPC static Shop definition missing")?,
                )?;
            let action = if bootstrap_shop {
                A::FreezeBootstrapIdle {
                    source,
                    operation,
                    event: binding.invocation,
                }
            } else {
                A::FreezeIdle { source, operation }
            };
            let queued = if bootstrap_shop {
                self.npc.coordinator.enqueue_retained(source, action)
            } else {
                self.npc.coordinator.enqueue(source, action)
            };
            queued.map_err(|_| "NPC terminal capture queue")?;
            self.npc.idle.insert(
                source,
                Idle {
                    operation,
                    checkpoint: None,
                    pending: None,
                    releasing: false,
                    archived: false,
                    committed: false,
                    cleanup: false,
                    cleaning: false,
                },
            );
        }
        self.npc.terminal_cursor = self
            .npc
            .terminal_cursor
            .wrapping_add(self.limits.work_per_poll)
            % 4096;
        for (&source, idle) in &mut self.npc.idle {
            if idle.cleanup && !idle.cleaning && !self.npc.coordinator.busy(source) {
                if self.npc.notifications_drained
                    && !self
                        .npc
                        .notifications
                        .iter()
                        .any(|n| n.context.source == source)
                {
                    self.npc
                        .coordinator
                        .enqueue_retained(source, A::ReleaseArchive { source })
                        .map_err(|_| "NPC archive release delivery pressure")?;
                    idle.cleaning = true;
                }
                continue;
            }
            if self.npc.coordinator.busy(source) || idle.releasing || idle.cleaning {
                continue;
            }
            if idle.pending.is_none()
                && let Some(checkpoint) = idle.checkpoint.take()
            {
                let (binding, version) = self
                    .npc
                    .coordinator
                    .binding(source)
                    .ok_or("NPC terminal source binding")?;
                match freeze_terminal_checkpoint(
                    binding,
                    version,
                    self.bootstrap.world_owner.epoch(),
                    checkpoint.clone(),
                ) {
                    Ok(mut pending) => {
                        let frozen = super::source_inventory::prepare(
                            &mut self.npc.source_inventory,
                            self.world.as_ref().map(|w| &w.regions),
                            &self.npc.definitions,
                            checkpoint.inventory.as_ref(),
                            self.bootstrap.world_owner.epoch(),
                        )
                        .and_then(|frozen| {
                            if let Some(frozen) = frozen {
                                pending
                                    .attach_source_inventory(&frozen)
                                    .map_err(|e| e.to_string())?;
                            }
                            Ok(())
                        });
                        if let Err(error) = frozen {
                            idle.checkpoint = Some(checkpoint);
                            return Err(error);
                        }
                        idle.pending = Some(pending);
                    }
                    Err(error) => {
                        idle.checkpoint = Some(checkpoint);
                        return Err(error.to_string());
                    }
                }
            }
            if let Some(pending) = idle.pending.take()
                && let Err(pending) =
                    self.npc
                        .coordinator
                        .enqueue_checkpoint(source, idle.operation, pending)
            {
                idle.pending = Some(*pending);
            }
        }
        Ok(())
    }
}
