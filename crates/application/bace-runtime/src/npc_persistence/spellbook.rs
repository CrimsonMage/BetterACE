//! Canonical spellbook and the adopted TeachSpell row share one durable stage.
use super::{NpcCheckpointBinding, PendingNpcStage, freeze_checkpoint, stage::stage_id};
use bace_gameplay_api::{NpcCompletion, NpcOperation, NpcRewardKind};
use bace_persistence::{
    CharacterLease, NpcStageOperation, NpcWorkflowUpdate, OwnershipState, PlacementOperation,
    SaveSnapshot,
};
use bace_simulation::{NpcEffect, NpcSourceCheckpoint, NpcSpellbookTicket};
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};
pub struct NpcSpellbookStageInput<'a> {
    pub binding: NpcCheckpointBinding,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub ticket: &'a NpcSpellbookTicket,
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub player: &'a PlayerSaveV6,
    pub player_version: i64,
    pub lease: CharacterLease,
}
pub fn freeze_spellbook_stage(
    input: NpcSpellbookStageInput<'_>,
) -> Result<PendingNpcStage, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC TeachSpell stage binding");
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
        || input.player.player.entity.mutation_revision != ticket.before_revision
        || ticket.spell == 0
        || ticket.spell > 65535
        || ticket.before.len() > 4096
        || ticket.after.len() > 4096
        || !matches!(ticket.npc.effect,NpcEffect::Service(NpcOperation::Reward{kind:NpcRewardKind::TeachSpell,amount,..})if amount==i64::from(ticket.spell))
        || !input
            .committed_checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == ticket.npc && p.adopted && p.completion == completion)
    {
        return Err(invalid());
    }
    let before = input
        .player
        .player
        .entity
        .state
        .properties
        .spell_book
        .iter()
        .map(|p| u32::try_from(p.id).map_err(|_| invalid()))
        .collect::<Result<Vec<_>, _>>()?;
    if before != ticket.before {
        return Err(invalid());
    }
    let mut after = before.clone();
    if !after.contains(&ticket.spell) {
        after.push(ticket.spell);
    }
    if after != ticket.after
        || ticket
            .before_revision
            .checked_add(u64::from(before != after))
            != Some(ticket.after_revision)
    {
        return Err(invalid());
    }
    let mut player = input.player.clone();
    player.player.entity.mutation_revision = ticket.after_revision;
    if !before.contains(&ticket.spell) {
        player
            .player
            .entity
            .state
            .properties
            .spell_book
            .push(bace_content::Property {
                id: ticket.spell as i32,
                value: 1.0,
            });
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
                    mutation_revision: ticket.after_revision,
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
