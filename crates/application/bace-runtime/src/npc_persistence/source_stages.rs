//! Deleted-source descriptors and post-destruction source property changes are
//! journaled with the same canonical full-source checkpoint as every other row.
use super::{NpcCheckpointBinding, PendingNpcStage, freeze_checkpoint, stage::stage_id};
use crate::game_inventory::{InventoryFreezeError, InventoryFreezeInput};
use bace_gameplay_api::{NpcCompletion, NpcOperation};
use bace_persistence::{
    CharacterLease, NpcStageOperation, NpcWorkflowUpdate, OwnershipState, PlacementOperation,
};
use bace_simulation::{
    InventoryReceipt, NpcDeleteSourceTicket, NpcEffect, NpcProposal, NpcSourceCheckpoint,
};
use bace_storage_codec::SaveCodecError;
pub struct NpcDeleteSourceStageInput<'a> {
    pub binding: NpcCheckpointBinding,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub ticket: &'a NpcDeleteSourceTicket,
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub retirement: Option<InventoryFreezeInput<'a>>,
    pub leases: Vec<CharacterLease>,
}
pub struct NpcDeleteSourceStage {
    pub pending: PendingNpcStage,
    pub receipt: Option<InventoryReceipt>,
}
pub fn freeze_delete_source_stage(
    input: NpcDeleteSourceStageInput<'_>,
) -> Result<NpcDeleteSourceStage, InventoryFreezeError> {
    let ticket = input.ticket;
    let completion = NpcCompletion::Applied { post_delay: 0.0 };
    if input.binding.source != ticket.npc.context.source.0
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || !matches!(
            ticket.npc.effect,
            NpcEffect::Service(NpcOperation::DeleteSelf)
        )
        || input.committed_checkpoint.archive.as_ref() != Some(&ticket.archive)
        || !input
            .committed_checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == ticket.npc && p.adopted && p.completion == completion)
    {
        return Err(InventoryFreezeError::Identity);
    }
    let (mut inventory, receipt) = match (&ticket.retirement, input.retirement) {
        (Some(retirement), Some(retirement_input)) => {
            let frozen = crate::generated_retirement::freeze_npc_retirement_operation(
                retirement,
                retirement_input,
                input.world_epoch,
                ticket.npc.ticket,
            )?;
            (
                frozen.inventory,
                Some(InventoryReceipt {
                    operation: retirement.inventory.operation,
                    revisions: retirement
                        .inventory
                        .proposal
                        .changes
                        .iter()
                        .map(|c| (c.after.id, c.after.revision))
                        .collect(),
                }),
            )
        }
        (None, None) => (
            PlacementOperation {
                operation_id: String::new(),
                snapshots: vec![],
                participants: vec![ticket.npc.context.source.0],
                leases: vec![],
                changes: vec![],
                storage_views: vec![],
            },
            None,
        ),
        _ => return Err(InventoryFreezeError::Identity),
    };
    for lease in input.leases {
        if lease.state != OwnershipState::Online || lease.epoch <= 0 {
            return Err(InventoryFreezeError::Identity);
        }
        if !inventory.leases.contains(&lease) {
            if inventory
                .leases
                .iter()
                .any(|l| l.character_id == lease.character_id)
            {
                return Err(InventoryFreezeError::Identity);
            }
            inventory.leases.push(lease);
        }
        inventory.participants.push(lease.character_id);
    }
    inventory.participants.push(ticket.npc.context.source.0);
    inventory.participants.sort_unstable();
    inventory.participants.dedup();
    if inventory.participants.len() > 1024 || inventory.leases.len() > 1024 {
        return Err(InventoryFreezeError::Capacity);
    }
    inventory.operation_id = stage_id(input.binding, input.workflow_version as u64);
    let checkpoint = freeze_checkpoint(
        input.binding,
        input.workflow_version as u64,
        input.committed_checkpoint,
    )?;
    Ok(NpcDeleteSourceStage {
        pending: PendingNpcStage::from_frozen_service(
            NpcStageOperation {
                inventory,
                workflow: NpcWorkflowUpdate {
                    invocation: input.binding.invocation,
                    world_epoch: input.world_epoch,
                    expected_version: input.workflow_version,
                    checkpoint: checkpoint.encode()?,
                },
            },
            ticket.npc.clone(),
            completion,
        ),
        receipt,
    })
}
pub struct NpcArchiveStageInput {
    pub binding: NpcCheckpointBinding,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub proposal: NpcProposal,
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub leases: Vec<CharacterLease>,
}
pub fn freeze_archive_stage(
    input: NpcArchiveStageInput,
) -> Result<PendingNpcStage, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC source state stage");
    let completion = NpcCompletion::Applied { post_delay: 0.0 };
    let actor = match &input.proposal.effect {
        NpcEffect::Property {
            actor,
            aggregate: None,
            change,
        } => {
            let properties = input
                .committed_checkpoint
                .archive
                .as_ref()
                .map(|a| &a.properties)
                .or(input.committed_checkpoint.properties.as_ref())
                .ok_or_else(invalid)?;
            if properties.revision() != change.after_revision
                || properties.get(change.family, change.stat) != change.after.as_ref()
            {
                return Err(invalid());
            }
            *actor
        }
        NpcEffect::Quest {
            actor,
            aggregate: None,
            change,
        } => {
            let (revision, rows) = input
                .committed_checkpoint
                .source_quests
                .as_ref()
                .ok_or_else(invalid)?;
            if *revision != change.after_revision
                || rows
                    .iter()
                    .find(|(name, _)| name == &change.name)
                    .map(|(_, value)| value)
                    != change.after.as_ref()
            {
                return Err(invalid());
            }
            *actor
        }
        _ => return Err(invalid()),
    };
    if actor.0 != input.binding.source
        || input.committed_checkpoint.source != actor
        || input.proposal.context.source != actor
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || !input
            .committed_checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == input.proposal && p.adopted && p.completion == completion)
        || input.leases.len() > 1024
        || input
            .leases
            .iter()
            .any(|l| l.epoch <= 0 || l.state != OwnershipState::Online)
    {
        return Err(invalid());
    }
    let checkpoint = freeze_checkpoint(
        input.binding,
        input.workflow_version as u64,
        input.committed_checkpoint,
    )?;
    let mut participants: Vec<_> = input
        .leases
        .iter()
        .map(|l| l.character_id)
        .chain([actor.0])
        .collect();
    participants.sort_unstable();
    participants.dedup();
    Ok(PendingNpcStage::from_frozen_service(
        NpcStageOperation {
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
        },
        input.proposal,
        completion,
    ))
}
