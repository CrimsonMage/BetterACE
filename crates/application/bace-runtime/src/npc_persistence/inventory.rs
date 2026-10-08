//! Inventory rows commit their exact item proposal and adopted workflow marker
//! together. A replay restores the accepted inventory before resuming the row.
use super::{NpcCheckpointBinding, PendingNpcStage, freeze_checkpoint, stage::stage_id};
use crate::game_inventory::{InventoryFreezeError, InventoryFreezeInput, freeze_inventory};
use bace_gameplay_api::{NpcCompletion, NpcOperation};
use bace_persistence::{NpcStageOperation, NpcWorkflowUpdate, OwnershipState};
use bace_simulation::{NpcEffect, NpcInventoryTicket, NpcSourceCheckpoint};

pub struct NpcInventoryStageInput<'a> {
    pub binding: NpcCheckpointBinding,
    pub stage: u64,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub ticket: &'a NpcInventoryTicket,
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub inventory: InventoryFreezeInput<'a>,
}
pub fn freeze_inventory_stage(
    input: NpcInventoryStageInput<'_>,
) -> Result<PendingNpcStage, InventoryFreezeError> {
    let completion = NpcCompletion::Applied { post_delay: 0.0 };
    let npc = &input.ticket.npc;
    if input.stage != input.workflow_version as u64
        || input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.binding.source != npc.context.source.0
        || input.ticket.inventory.operation == 0
        || npc.context.target != Some(input.ticket.inventory.actor)
        || input.inventory.proposal != &input.ticket.inventory.proposal
        || !matches!(
            npc.effect,
            NpcEffect::Service(NpcOperation::Give { .. } | NpcOperation::Take { .. })
        )
        || !input.inventory.leases.iter().any(|l| {
            l.character_id == input.ticket.inventory.actor.0
                && l.state == OwnershipState::Online
                && l.epoch > 0
        })
        || !input
            .committed_checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == *npc && p.adopted && p.completion == completion)
    {
        return Err(InventoryFreezeError::Identity);
    }
    let checkpoint = freeze_checkpoint(input.binding, input.stage, input.committed_checkpoint)?;
    let operation_id = stage_id(input.binding, input.stage);
    let mut inventory = freeze_inventory(InventoryFreezeInput {
        operation_id: &operation_id,
        ..input.inventory
    })?;
    inventory.participants.push(npc.context.source.0);
    inventory.participants.sort_unstable();
    inventory.participants.dedup();
    if inventory.participants.len() > 1024 {
        return Err(InventoryFreezeError::Capacity);
    }
    Ok(PendingNpcStage::from_frozen_service(
        NpcStageOperation {
            inventory,
            workflow: NpcWorkflowUpdate {
                invocation: input.binding.invocation,
                world_epoch: input.world_epoch,
                expected_version: input.workflow_version,
                checkpoint: checkpoint.encode()?,
            },
        },
        npc.clone(),
        completion,
    ))
}
