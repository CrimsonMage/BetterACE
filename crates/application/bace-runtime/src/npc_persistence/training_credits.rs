//! Both AddSkillCredits counters and the source checkpoint commit together.
use super::{NpcCheckpointBinding, PendingNpcStage, freeze_checkpoint, stage::stage_id};
use bace_gameplay_api::{NpcCompletion, NpcOperation, NpcRewardKind};
use bace_persistence::{
    CharacterLease, NpcStageOperation, NpcWorkflowUpdate, OwnershipState, PlacementOperation,
    SaveSnapshot,
};
use bace_simulation::{NpcEffect, NpcSourceCheckpoint, NpcTrainingCreditTicket};
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};
pub struct NpcTrainingCreditStageInput<'a> {
    pub binding: NpcCheckpointBinding,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub ticket: &'a NpcTrainingCreditTicket,
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub player: &'a PlayerSaveV6,
    pub player_version: i64,
    pub lease: CharacterLease,
}
pub fn freeze_training_credit_stage(
    input: NpcTrainingCreditStageInput<'_>,
) -> Result<PendingNpcStage, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC training credits stage binding");
    let ticket = input.ticket;
    let c = &ticket.change;
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
        || input.player.player.entity.mutation_revision != c.before_revision
        || !matches!(ticket.npc.effect,NpcEffect::Service(NpcOperation::Reward{kind:NpcRewardKind::TrainingCredits,amount,..})if amount==i64::from(c.amount))
        || !input
            .committed_checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == ticket.npc && p.adopted && p.completion == completion)
    {
        return Err(invalid());
    }
    let props = &input.player.player.entity.state.properties;
    let scalar = |id| props.ints.iter().find(|p| p.id == id).map(|p| p.value);
    let available = scalar(24)
        .map(u32::try_from)
        .transpose()
        .map_err(|_| invalid())?;
    if scalar(23) != c.before_total
        || available != c.before_available
        || c.before_total.map(|n| n.checked_add(c.amount)) != c.after_total.map(Some)
        || c.before_available.map(|n| {
            u32::try_from(i64::from(n) + i64::from(c.amount))
                .ok()
                .filter(|v| *v <= i32::MAX as u32)
        }) != c.after_available.map(Some)
        || c.before_revision.checked_add(u64::from(
            c.before_available != c.after_available || c.before_total != c.after_total,
        )) != Some(c.after_revision)
    {
        return Err(invalid());
    }
    let mut player = input.player.clone();
    player.player.entity.mutation_revision = c.after_revision;
    for (id, value) in [
        (23, c.after_total),
        (24, c.after_available.map(|v| v as i32)),
    ] {
        if let Some(value) = value {
            let fields = &mut player.player.entity.state.properties.ints;
            if let Some(p) = fields.iter_mut().find(|p| p.id == id) {
                p.value = value;
            } else {
                fields.push(bace_content::Property { id, value });
                fields.sort_by_key(|p| p.id);
            }
        }
    }
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
                    mutation_revision: c.after_revision,
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
