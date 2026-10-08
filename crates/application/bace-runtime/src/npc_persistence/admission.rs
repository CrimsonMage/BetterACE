//! Durable admission of independently scheduled source chains. This stage
//! advances the workflow only; it never claims that the delegated effect ran.
use super::{NpcCheckpointBinding, PendingNpcStage, freeze_checkpoint, stage::stage_id};
use bace_gameplay_api::NpcCompletion;
use bace_persistence::{CharacterLease, NpcStageOperation, NpcWorkflowUpdate, PlacementOperation};
use bace_simulation::{NpcEffect, NpcProposal, NpcSourceCheckpoint};
use bace_storage_codec::SaveCodecError;
pub struct NpcServiceAdmissionInput {
    pub binding: NpcCheckpointBinding,
    pub stage: u64,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub proposal: NpcProposal,
    pub post_delay: f64,
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub leases: Vec<CharacterLease>,
}
pub fn freeze_service_admission(
    input: NpcServiceAdmissionInput,
) -> Result<PendingNpcStage, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC detached service admission");
    if input.binding.source != input.proposal.context.source.0
        || input.proposal.ticket == 0
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || input.stage != input.workflow_version as u64
        || !input.post_delay.is_finite()
        || input.post_delay < 0.0
        || !matches!(input.proposal.effect, NpcEffect::Service(_))
        || input.leases.len() > 1024
        || !input.committed_checkpoint.pending.iter().any(|p| {
            p.proposal == input.proposal
                && p.detached
                && !p.adopted
                && p.completion
                    == NpcCompletion::Applied {
                        post_delay: input.post_delay,
                    }
        })
    {
        return Err(invalid());
    }
    let checkpoint = freeze_checkpoint(input.binding, input.stage, input.committed_checkpoint)?;
    let mut participants = vec![input.proposal.context.source.0];
    if let Some(target) = input.proposal.context.target {
        participants.push(target.0);
    }
    participants.sort_unstable();
    participants.dedup();
    if input
        .leases
        .iter()
        .any(|l| !participants.contains(&l.character_id) || l.epoch <= 0)
    {
        return Err(invalid());
    }
    let operation = NpcStageOperation {
        inventory: PlacementOperation {
            operation_id: stage_id(input.binding, input.stage),
            snapshots: vec![],
            participants,
            leases: input.leases,
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
    Ok(PendingNpcStage::from_frozen_admission(
        operation,
        input.proposal,
        input.post_delay,
    ))
}
