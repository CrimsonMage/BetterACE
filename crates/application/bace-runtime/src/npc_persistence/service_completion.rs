//! Terminal motion/cast receipts already come from their real domain owner.
//! This stage journals the continuation before releasing that retained receipt.
use super::{NpcCheckpointBinding, PendingNpcStage, freeze_checkpoint, stage::stage_id};
use bace_gameplay_api::{NpcCompletion, NpcOperation};
use bace_persistence::{CharacterLease, NpcStageOperation, NpcWorkflowUpdate, PlacementOperation};
use bace_simulation::{NpcEffect, NpcProposal, NpcSourceCheckpoint};
use bace_storage_codec::SaveCodecError;
pub struct NpcServiceCompletionInput {
    pub binding: NpcCheckpointBinding,
    pub workflow_version: i64,
    pub world_epoch: u64,
    pub proposal: NpcProposal,
    pub checkpoint: NpcSourceCheckpoint,
    pub leases: Vec<CharacterLease>,
}
pub fn freeze_service_completion(
    input: NpcServiceCompletionInput,
) -> Result<PendingNpcStage, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC terminal service checkpoint");
    let completion = NpcCompletion::Applied { post_delay: 0.0 };
    if !matches!(
        input.proposal.effect,
        NpcEffect::Service(NpcOperation::Cast { .. } | NpcOperation::Motion { .. })
    ) || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || input.leases.len() > 2
        || !input
            .checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == input.proposal && p.adopted && p.completion == completion)
    {
        return Err(invalid());
    }
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
    let checkpoint = freeze_checkpoint(
        input.binding,
        input.workflow_version as u64,
        input.checkpoint,
    )?;
    let operation = NpcStageOperation {
        inventory: PlacementOperation {
            operation_id: stage_id(input.binding, input.workflow_version as u64),
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
    Ok(PendingNpcStage::from_frozen_service(
        operation,
        input.proposal,
        completion,
    ))
}
