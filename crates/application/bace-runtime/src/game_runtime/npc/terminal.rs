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
            self.npc
                .coordinator
                .enqueue(source, A::FreezeIdle { source, operation })
                .map_err(|_| "NPC terminal capture queue")?;
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
