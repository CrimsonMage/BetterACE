//! Portal final destination and the source row's adoption marker commit as one
//! journaled operation. The existing portal owner applies the matching receipt.
use super::{NpcCheckpointBinding, PendingNpcStage, freeze_checkpoint, stage::stage_id};
use crate::portal_saves::{PortalSavePlayer, freeze_portal_service};
use bace_gameplay_api::NpcCompletion;
use bace_persistence::{NpcStageOperation, NpcWorkflowUpdate};
use bace_simulation::{
    NpcPortalTicket, NpcSourceCheckpoint, PortalServiceOrigin, PortalServiceReceipt,
};
use bace_storage_codec::SaveCodecError;
pub struct NpcTeleportStageInput<'a> {
    pub binding: NpcCheckpointBinding,
    pub world_epoch: u64,
    pub workflow_version: i64,
    pub ticket: &'a NpcPortalTicket,
    pub committed_checkpoint: NpcSourceCheckpoint,
    pub players: &'a [PortalSavePlayer<'a>],
}
pub struct NpcTeleportStage {
    pub pending: PendingNpcStage,
    pub receipt: PortalServiceReceipt,
}
pub fn freeze_teleport_stage(
    input: NpcTeleportStageInput<'_>,
) -> Result<NpcTeleportStage, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("NPC teleport stage binding");
    let ticket = input.ticket;
    let completion = NpcCompletion::Applied { post_delay: 0.0 };
    if input.binding.source != ticket.npc.context.source.0
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.workflow_version < 0
        || input.workflow_version == i64::MAX
        || ticket.portal.origin
            != (PortalServiceOrigin::Emote {
                ticket: ticket.npc.ticket,
            })
        || ticket.npc.context.target != Some(ticket.portal.actor)
        || !input
            .committed_checkpoint
            .pending
            .iter()
            .any(|p| p.proposal == ticket.npc && p.adopted && p.completion == completion)
    {
        return Err(invalid());
    }
    let mut frozen = freeze_portal_service(input.world_epoch, &ticket.portal, input.players)?;
    frozen.operation.operation_id = stage_id(input.binding, input.workflow_version as u64);
    frozen
        .operation
        .participants
        .push(ticket.npc.context.source.0);
    frozen.operation.participants.sort_unstable();
    frozen.operation.participants.dedup();
    let checkpoint = freeze_checkpoint(
        input.binding,
        input.workflow_version as u64,
        input.committed_checkpoint,
    )?;
    let pending = PendingNpcStage::from_frozen_service(
        NpcStageOperation {
            inventory: frozen.operation,
            workflow: NpcWorkflowUpdate {
                invocation: input.binding.invocation,
                world_epoch: input.world_epoch,
                expected_version: input.workflow_version,
                checkpoint: checkpoint.encode()?,
            },
        },
        ticket.npc.clone(),
        completion,
    );
    Ok(NpcTeleportStage {
        pending,
        receipt: frozen.receipt,
    })
}
