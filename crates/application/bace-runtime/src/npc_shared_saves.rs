//! Quest XP, its shared recipients and the source emote checkpoint commit together.
use crate::{
    allegiance_players::{AllegiancePlayerInput, freeze_allegiance_players},
    allegiance_saves::{AllegianceFreezeInput, freeze_allegiance},
    npc_persistence::{NpcCheckpointBinding, freeze_checkpoint},
};
use bace_persistence::{
    AllegiancePlacementOperation, NpcWorkflowUpdate, PlacementOperation, StoredAllegiance,
};
use bace_simulation::{AllegianceTicket, NpcSourceCheckpoint};
use bace_storage_codec::SaveCodecError;
pub struct NpcSharedFreezeInput<'a> {
    pub ticket: &'a AllegianceTicket,
    pub binding: NpcCheckpointBinding,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub checkpoint: NpcSourceCheckpoint,
    pub players: &'a [AllegiancePlayerInput<'a>],
    pub nodes: &'a [StoredAllegiance],
    pub metadata: &'a [StoredAllegiance],
    pub item_operations: &'a [(u64, PlacementOperation)],
}
pub fn freeze_npc_shared_experience(
    input: NpcSharedFreezeInput<'_>,
) -> Result<AllegiancePlacementOperation, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC shared XP checkpoint binding");
    let npc = input.ticket.npc.as_ref().ok_or_else(invalid)?;
    if input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || input.world_epoch == 0
        || input.binding.source != npc.context.source.0
        || !input
            .checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == *npc && p.adopted && p.detached)
    {
        return Err(invalid());
    }
    let frozen = freeze_allegiance_players(input.ticket, input.players)?;
    let mut allegiance = freeze_allegiance(AllegianceFreezeInput {
        world_epoch: input.world_epoch,
        operation: input.ticket.operation,
        patch: &input.ticket.patch,
        stored_nodes: input.nodes,
        stored_metadata: input.metadata,
        players: &[],
        leases: &frozen.leases,
    })?;
    let stage = input.workflow_version as u64;
    let checkpoint = freeze_checkpoint(input.binding, stage, input.checkpoint)?;
    let id = format!(
        "npc-shared:{}:{}:{}",
        input.world_epoch,
        u128::from_le_bytes(input.binding.invocation),
        stage
    );
    allegiance.operation_id = id.clone();
    let mut participants: Vec<_> = frozen
        .snapshots
        .iter()
        .map(|s| s.object_id)
        .chain(allegiance.nodes.iter().map(|n| n.character))
        .chain(allegiance.metadata.iter().map(|n| n.character))
        .chain([npc.context.source.0])
        .collect();
    participants.sort_unstable();
    participants.dedup();
    Ok(AllegiancePlacementOperation {
        placement: crate::item_reward_join::join_allegiance_item_operations(
            PlacementOperation {
                operation_id: id,
                snapshots: frozen.snapshots,
                participants,
                leases: frozen.leases,
                changes: vec![],
                storage_views: vec![],
            },
            input.ticket,
            input.item_operations,
        )?,
        allegiance,
        world_epoch: input.world_epoch,
        workflow: Some(NpcWorkflowUpdate {
            invocation: input.binding.invocation,
            world_epoch: input.world_epoch,
            expected_version: input.workflow_version,
            checkpoint: checkpoint.encode()?,
        }),
    })
}
