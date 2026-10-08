//! Final idle VM checkpoint. Completion retires the source's active journal
//! head without discarding a deleted-source tombstone or its monotone version.
use super::{NpcCheckpointBinding, freeze_checkpoint, stage::stage_id};
use crate::saves::{SaveFailure, SaveHandle, SaveSubmitError, SaveTicket, WriteOutcome};
use bace_persistence::{
    NpcStageOperation, NpcWorkflowUpdate, OperationOutcome, PlacementOperation,
};
use bace_simulation::NpcSourceCheckpoint;
use bace_storage_codec::SaveCodecError;
pub struct PendingNpcCheckpoint {
    operation: NpcStageOperation,
    receiver: Option<SaveTicket>,
    uncertain: bool,
    terminal: bool,
}
#[derive(Debug)]
pub enum NpcCheckpointResolution {
    Committed,
    Rejected(String),
    Uncertain(String),
}
impl PendingNpcCheckpoint {
    pub(crate) fn attach_source_inventory(
        &mut self,
        source: &super::FrozenNpcSourceInventory,
    ) -> Result<(), bace_storage_codec::SaveCodecError> {
        if self.receiver.is_some() || self.terminal || self.uncertain {
            return Err(bace_storage_codec::SaveCodecError::Invalid(
                "NPC source join after submission",
            ));
        }
        source.attach(&mut self.operation)
    }
    pub fn operation(&self) -> &NpcStageOperation {
        &self.operation
    }
    pub fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        if !super::source_inventory::joined(&self.operation) {
            return Err(SaveSubmitError::Invalid);
        }
        if self.receiver.is_some() || self.terminal {
            return Err(SaveSubmitError::Invalid);
        }
        self.receiver = Some(saves.try_npc_stage(&self.operation)?);
        Ok(())
    }
    pub fn poll(&mut self) -> Option<NpcCheckpointResolution> {
        let report = match self.receiver.as_mut()?.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.receiver = None;
                self.uncertain = true;
                return Some(NpcCheckpointResolution::Uncertain(
                    "NPC terminal checkpoint response lost".into(),
                ));
            }
        };
        self.receiver = None;
        match report.result {
            Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)) => {
                self.terminal = true;
                Some(NpcCheckpointResolution::Committed)
            }
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(mut acks))) => {
                acks.sort_by_key(|a| a.object_id);
                let mut expected: Vec<_> = self
                    .operation
                    .inventory
                    .snapshots
                    .iter()
                    .map(|s| bace_persistence::SaveAck {
                        object_id: s.object_id,
                        mutation_revision: s.mutation_revision,
                        persisted_version: s.expected_version + 1,
                    })
                    .collect();
                expected.sort_by_key(|a| a.object_id);
                if acks != expected {
                    self.uncertain = true;
                    return Some(NpcCheckpointResolution::Uncertain(
                        "NPC source checkpoint receipt mismatch".into(),
                    ));
                }
                self.terminal = true;
                Some(NpcCheckpointResolution::Committed)
            }
            Err(SaveFailure::Storage {
                message,
                uncertain: false,
            }) if !self.uncertain => {
                self.terminal = true;
                Some(NpcCheckpointResolution::Rejected(message))
            }
            other => {
                self.uncertain = true;
                Some(NpcCheckpointResolution::Uncertain(format!(
                    "NPC terminal checkpoint unresolved: {other:?}"
                )))
            }
        }
    }
}
pub fn freeze_terminal_checkpoint(
    binding: NpcCheckpointBinding,
    workflow_version: i64,
    world_epoch: u64,
    checkpoint: NpcSourceCheckpoint,
) -> Result<PendingNpcCheckpoint, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC terminal checkpoint identity");
    if world_epoch == 0
        || world_epoch > i64::MAX as u64
        || workflow_version < 0
        || workflow_version == i64::MAX
    {
        return Err(invalid());
    }
    let checkpoint = freeze_checkpoint(binding, workflow_version as u64, checkpoint)?;
    if !checkpoint.completed {
        return Err(invalid());
    }
    Ok(PendingNpcCheckpoint {
        operation: NpcStageOperation {
            inventory: PlacementOperation {
                operation_id: stage_id(binding, workflow_version as u64),
                snapshots: vec![],
                participants: vec![binding.source],
                leases: vec![],
                changes: vec![],
                storage_views: vec![],
            },
            workflow: NpcWorkflowUpdate {
                invocation: binding.invocation,
                world_epoch,
                expected_version: workflow_version,
                checkpoint: checkpoint.encode()?,
            },
        },
        receiver: None,
        uncertain: false,
        terminal: false,
    })
}
