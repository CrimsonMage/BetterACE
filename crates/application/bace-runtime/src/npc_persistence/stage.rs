//! Exact durable stage bytes and correlated receipts. An uncertain response never
//! authorizes rollback or a newly prepared reward.
use super::{NpcCheckpointBinding, freeze_checkpoint, freeze_player_effect};
use crate::saves::{
    SaveFailure, SaveHandle, SaveReport, SaveSubmitError, SaveTicket, WriteOutcome,
};
use bace_gameplay_api::NpcCompletion;
use bace_persistence::{
    CharacterLease, NpcStageOperation, NpcWorkflowUpdate, OperationOutcome, OwnershipState,
    PlacementOperation, SaveAck, SaveSnapshot,
};
use bace_simulation::{NpcProposal, NpcSourceCheckpoint};
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};

pub struct NpcPlayerStageInput<'a> {
    pub binding: NpcCheckpointBinding,
    pub stage: u64,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub proposal: NpcProposal,
    pub completion: NpcCompletion,
    /// Preview from the simulation owner. The selected row is marked adopted,
    /// but live gameplay has not yet been mutated.
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub player: &'a PlayerSaveV6,
    pub player_version: i64,
    pub lease: CharacterLease,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NpcStageAdoption {
    Effect(NpcCompletion),
    QueuedExperience,
    DetachedService { post_delay: f64 },
}
#[derive(Debug)]
pub enum NpcStageResolution {
    Committed {
        proposal: NpcProposal,
        adoption: NpcStageAdoption,
        acknowledgments: Vec<SaveAck>,
    },
    Rejected {
        proposal: NpcProposal,
        failure: SaveFailure,
    },
    Uncertain {
        message: String,
    },
}
/// Keep this owner until a terminal receipt. Retrying submits the same operation
/// and checkpoint bytes, including the original execution epoch.
pub struct PendingNpcStage {
    operation: NpcStageOperation,
    proposal: NpcProposal,
    adoption: NpcStageAdoption,
    receiver: Option<SaveTicket>,
    uncertain: bool,
    terminal: bool,
}
impl PendingNpcStage {
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
    pub(crate) fn join_captured_inventory(
        &mut self,
        rows: Vec<SaveSnapshot>,
    ) -> Result<(), SaveCodecError> {
        let invalid = || SaveCodecError::Invalid("NPC captured inventory join");
        if self.receiver.is_some() || self.terminal || self.uncertain {
            return Err(invalid());
        }
        let mut snapshots = self.operation.inventory.snapshots.clone();
        let mut participants: std::collections::BTreeSet<_> = self
            .operation
            .inventory
            .participants
            .iter()
            .copied()
            .collect();
        for row in rows {
            if let Some(old) = snapshots.iter().find(|s| s.object_id == row.object_id) {
                if old.mutation_revision != row.mutation_revision
                    || old.expected_version != row.expected_version
                    || old.bytes != row.bytes
                {
                    return Err(invalid());
                }
                continue;
            }
            let item = bace_storage_codec::ItemSaveV5::decode(&row.bytes)?;
            if item.entity.object_id != row.object_id
                || item.entity.mutation_revision != row.mutation_revision
                || row.expected_version <= 0
                || row.expected_version == i64::MAX
            {
                return Err(invalid());
            }
            participants.insert(row.object_id);
            snapshots.push(row);
        }
        if snapshots.len() > 1024
            || participants.len() > 1024
            || snapshots.iter().map(|s| s.bytes.len()).sum::<usize>() > 64 * 1024 * 1024
        {
            return Err(invalid());
        }
        snapshots.sort_by_key(|s| s.object_id);
        self.operation.inventory.snapshots = snapshots;
        self.operation.inventory.participants = participants.into_iter().collect();
        Ok(())
    }

    pub(super) fn from_frozen_admission(
        operation: NpcStageOperation,
        proposal: NpcProposal,
        post_delay: f64,
    ) -> Self {
        Self {
            operation,
            proposal,
            adoption: NpcStageAdoption::DetachedService { post_delay },
            receiver: None,
            uncertain: false,
            terminal: false,
        }
    }
    pub(super) fn from_frozen_service(
        operation: NpcStageOperation,
        proposal: NpcProposal,
        completion: NpcCompletion,
    ) -> Self {
        Self {
            operation,
            proposal,
            adoption: NpcStageAdoption::Effect(completion),
            receiver: None,
            uncertain: false,
            terminal: false,
        }
    }
    pub fn proposal(&self) -> &NpcProposal {
        &self.proposal
    }
    pub fn operation(&self) -> &NpcStageOperation {
        &self.operation
    }
    pub fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        if !super::source_inventory::joined(&self.operation) {
            return Err(SaveSubmitError::Invalid);
        }
        if self.terminal || self.receiver.is_some() {
            return Err(SaveSubmitError::Invalid);
        }
        self.receiver = Some(saves.try_npc_stage(&self.operation)?);
        Ok(())
    }
    pub fn poll(&mut self) -> Option<NpcStageResolution> {
        let receiver = self.receiver.as_mut()?;
        let report = match receiver.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.receiver = None;
                self.uncertain = true;
                return Some(NpcStageResolution::Uncertain {
                    message: "NPC stage reply closed; durable outcome unresolved".into(),
                });
            }
        };
        self.receiver = None;
        let result = self.resolve(report);
        if matches!(result, NpcStageResolution::Uncertain { .. }) {
            self.uncertain = true;
        }
        Some(result)
    }
    fn resolve(&mut self, report: SaveReport) -> NpcStageResolution {
        let expected: Vec<_> = self
            .operation
            .inventory
            .snapshots
            .iter()
            .map(|s| SaveAck {
                object_id: s.object_id,
                mutation_revision: s.mutation_revision,
                persisted_version: s.expected_version + 1,
            })
            .collect();
        let acknowledgments = match report.result {
            Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)) => expected,
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(mut acks))) => {
                let mut expected = expected;
                expected.sort_by_key(|a| a.object_id);
                acks.sort_by_key(|a| a.object_id);
                if acks != expected {
                    return NpcStageResolution::Uncertain {
                        message: "NPC stage acknowledgments do not match frozen participants"
                            .into(),
                    };
                }
                acks
            }
            Err(
                failure @ SaveFailure::Storage {
                    uncertain: false, ..
                },
            ) if !self.uncertain => {
                self.terminal = true;
                return NpcStageResolution::Rejected {
                    proposal: self.proposal.clone(),
                    failure,
                };
            }
            Err(failure) => {
                return NpcStageResolution::Uncertain {
                    message: failure.to_string(),
                };
            }
            Ok(_) => {
                return NpcStageResolution::Uncertain {
                    message: "unexpected NPC stage result".into(),
                };
            }
        };
        self.terminal = true;
        NpcStageResolution::Committed {
            proposal: self.proposal.clone(),
            adoption: self.adoption,
            acknowledgments,
        }
    }
}
pub fn freeze_player_stage(
    input: NpcPlayerStageInput<'_>,
) -> Result<PendingNpcStage, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC durable player stage binding");
    if input.proposal.ticket == 0
        || input.binding.source != input.proposal.context.source.0
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || input.player_version <= 0
        || input.player_version == i64::MAX
        || input.lease.character_id != input.player.player.entity.object_id
        || input.lease.epoch <= 0
        || input.lease.state != OwnershipState::Online
        || input.stage != input.workflow_version as u64
        || !input
            .committed_checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == input.proposal && p.adopted && p.completion == input.completion)
    {
        return Err(invalid());
    }
    let player = freeze_player_effect(input.player, &input.proposal.effect)?;
    let checkpoint = freeze_checkpoint(input.binding, input.stage, input.committed_checkpoint)?;
    let operation_id = stage_id(input.binding, input.stage);
    let mut participants = vec![input.binding.source, player.player.entity.object_id];
    participants.sort_unstable();
    participants.dedup();
    let operation = NpcStageOperation {
        inventory: PlacementOperation {
            operation_id,
            snapshots: vec![SaveSnapshot {
                object_id: player.player.entity.object_id,
                mutation_revision: player.player.entity.mutation_revision,
                expected_version: input.player_version,
                bytes: player.encode()?,
            }],
            participants,
            leases: vec![input.lease],
            changes: vec![],
            storage_views: vec![],
        },
        workflow: NpcWorkflowUpdate {
            invocation: input.binding.invocation,
            world_epoch: input.world_epoch,
            expected_version: input.workflow_version,
            checkpoint: checkpoint.encode()?,
        },
    };
    Ok(PendingNpcStage {
        operation,
        proposal: input.proposal,
        adoption: NpcStageAdoption::Effect(input.completion),
        receiver: None,
        uncertain: false,
        terminal: false,
    })
}

/// Admission only: queued source XP becomes durable without changing the player.
pub struct NpcExperienceAdmissionInput {
    pub binding: NpcCheckpointBinding,
    pub stage: u64,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub proposal: NpcProposal,
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub lease: CharacterLease,
}
pub fn freeze_experience_admission(
    input: NpcExperienceAdmissionInput,
) -> Result<PendingNpcStage, SaveCodecError> {
    use bace_simulation::{NpcEffect, NpcQueuedExperiencePhase as Phase};
    let invalid = || SaveCodecError::Invalid("NPC queued experience admission binding");
    let (actor, amount, share) = match &input.proposal.effect {
        NpcEffect::QueuedExperience {
            actor,
            amount,
            share,
            phase: Phase::AwaitingAdmission,
        } => (*actor, *amount, *share),
        _ => return Err(invalid()),
    };
    if input.proposal.ticket == 0
        || input.binding.source != input.proposal.context.source.0
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || input.stage != input.workflow_version as u64
        || input.lease.character_id != actor.0
        || input.lease.epoch <= 0
        || input.lease.state != OwnershipState::Online
        || !input.committed_checkpoint.pending.iter().any(|p| {
            p.proposal.ticket == input.proposal.ticket
                && p.proposal.context == input.proposal.context
                && !p.adopted
                && p.detached
                && p.proposal.effect
                    == NpcEffect::QueuedExperience {
                        actor,
                        amount,
                        share,
                        phase: Phase::Ready,
                    }
        })
    {
        return Err(invalid());
    }
    let checkpoint = freeze_checkpoint(input.binding, input.stage, input.committed_checkpoint)?;
    let mut participants = vec![input.binding.source, actor.0];
    participants.sort_unstable();
    participants.dedup();
    let operation = NpcStageOperation {
        inventory: PlacementOperation {
            operation_id: stage_id(input.binding, input.stage),
            snapshots: vec![],
            participants,
            leases: vec![input.lease],
            changes: vec![],
            storage_views: vec![],
        },
        workflow: NpcWorkflowUpdate {
            invocation: input.binding.invocation,
            world_epoch: input.world_epoch,
            expected_version: input.workflow_version,
            checkpoint: checkpoint.encode()?,
        },
    };
    Ok(PendingNpcStage {
        operation,
        proposal: input.proposal,
        adoption: NpcStageAdoption::QueuedExperience,
        receiver: None,
        uncertain: false,
        terminal: false,
    })
}
pub(super) fn stage_id(binding: NpcCheckpointBinding, stage: u64) -> String {
    use std::fmt::Write;
    let mut id = String::from("npc:");
    for byte in binding.invocation {
        write!(id, "{byte:02x}").expect("String write");
    }
    write!(id, ":{stage}").expect("String write");
    id
}

#[cfg(test)]
mod tests;
