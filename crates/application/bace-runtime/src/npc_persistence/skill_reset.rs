//! UntrainSkill reuses the frozen skill mutation codec and the NPC stage journal.
use super::{NpcCheckpointBinding, PendingNpcStage, freeze_checkpoint, stage::stage_id};
use bace_gameplay_api::{NpcCompletion, NpcOperation, NpcRewardKind, ProgressionTarget};
use bace_persistence::{
    CharacterLease, NpcStageOperation, NpcWorkflowUpdate, OwnershipState, PlacementOperation,
    SaveSnapshot,
};
use bace_simulation::{NpcEffect, NpcSkillResetTicket, NpcSourceCheckpoint};
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};
pub struct NpcSkillResetStageInput<'a> {
    pub binding: NpcCheckpointBinding,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub ticket: &'a NpcSkillResetTicket,
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub player: &'a PlayerSaveV6,
    pub player_version: i64,
    pub lease: CharacterLease,
}
pub fn freeze_skill_reset_stage(
    input: NpcSkillResetStageInput<'_>,
) -> Result<PendingNpcStage, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC skill reset stage binding");
    let ticket = input.ticket;
    let completion = NpcCompletion::Applied { post_delay: 0.0 };
    if input.binding.source != ticket.npc.context.source.0
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || input.player_version < 1
        || input.player_version == i64::MAX
        || input.lease.character_id != ticket.actor.0
        || input.lease.epoch <= 0
        || input.lease.state != OwnershipState::Online
        || input.player.player.entity.object_id != ticket.actor.0
        || !matches!(ticket.npc.effect,NpcEffect::Service(NpcOperation::Reward{kind:NpcRewardKind::UntrainSkill,stat:Some(skill),..})if ticket.change.before.target==ProgressionTarget::Skill(skill))
        || !input
            .committed_checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == ticket.npc && p.adopted && p.completion == completion)
    {
        return Err(invalid());
    }
    let player = crate::progression_saves::freeze_skill_change(
        input.player,
        ticket.change,
        ticket.before_revision,
    )
    .map_err(|_| invalid())?;
    let checkpoint = freeze_checkpoint(
        input.binding,
        input.workflow_version as u64,
        input.committed_checkpoint,
    )?;
    let mut participants = vec![ticket.actor.0, ticket.npc.context.source.0];
    participants.sort_unstable();
    participants.dedup();
    Ok(PendingNpcStage::from_frozen_service(
        NpcStageOperation {
            inventory: PlacementOperation {
                operation_id: stage_id(input.binding, input.workflow_version as u64),
                snapshots: vec![SaveSnapshot {
                    object_id: ticket.actor.0,
                    mutation_revision: ticket.change.revision,
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
        },
        ticket.npc.clone(),
        completion,
    ))
}
