//! Initial item acceptance and scheduled Give continuation commit atomically.
//! This is its own durable stage; later rewards are independent source stages.
#[cfg(test)]
mod tests;
use super::{NpcCheckpointBinding, freeze_checkpoint, stage::stage_id};
use crate::{
    game_inventory::{InventoryFreezeError, InventoryFreezeInput, freeze_inventory},
    saves::{SaveFailure, SaveHandle, SaveSubmitError, SaveTicket, WriteOutcome},
};
use bace_persistence::{
    NpcStageOperation, NpcWorkflowUpdate, OperationOutcome, OwnershipState, SaveAck,
};
use bace_simulation::{InventoryReceipt, NpcHandInTicket};
pub struct NpcHandInStageInput<'a> {
    pub binding: NpcCheckpointBinding,
    pub world_epoch: u64,
    pub ticket: &'a NpcHandInTicket,
    pub inventory: InventoryFreezeInput<'a>,
}
pub struct PendingNpcHandIn {
    operation: NpcStageOperation,
    ticket: NpcHandInTicket,
    receiver: Option<SaveTicket>,
    uncertain: bool,
    terminal: bool,
}
#[derive(Debug)]
pub enum NpcHandInResolution {
    Committed {
        ticket: NpcHandInTicket,
        receipt: InventoryReceipt,
        acknowledgments: Vec<SaveAck>,
    },
    Rejected {
        ticket: NpcHandInTicket,
        failure: SaveFailure,
    },
    Uncertain(String),
}
impl PendingNpcHandIn {
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
    pub fn poll(&mut self) -> Option<NpcHandInResolution> {
        let report = match self.receiver.as_mut()?.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.receiver = None;
                self.uncertain = true;
                return Some(NpcHandInResolution::Uncertain(
                    "NPC hand-in reply closed".into(),
                ));
            }
        };
        self.receiver = None;
        let mut expected: Vec<_> = self
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
        expected.sort_by_key(|a| a.object_id);
        let acknowledgments = match report.result {
            Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)) => expected,
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(mut actual))) => {
                actual.sort_by_key(|a| a.object_id);
                if actual != expected {
                    self.uncertain = true;
                    return Some(NpcHandInResolution::Uncertain(
                        "NPC hand-in receipt mismatch".into(),
                    ));
                }
                actual
            }
            Err(
                failure @ SaveFailure::Storage {
                    uncertain: false, ..
                },
            ) if !self.uncertain => {
                self.terminal = true;
                return Some(NpcHandInResolution::Rejected {
                    ticket: self.ticket.clone(),
                    failure,
                });
            }
            Err(failure) => {
                self.uncertain = true;
                return Some(NpcHandInResolution::Uncertain(failure.to_string()));
            }
            Ok(_) => {
                self.uncertain = true;
                return Some(NpcHandInResolution::Uncertain(
                    "unexpected NPC hand-in receipt".into(),
                ));
            }
        };
        self.terminal = true;
        Some(NpcHandInResolution::Committed {
            ticket: self.ticket.clone(),
            receipt: InventoryReceipt {
                operation: self.ticket.inventory.operation,
                revisions: self
                    .ticket
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .map(|c| (c.after.id, c.after.revision))
                    .collect(),
            },
            acknowledgments,
        })
    }
}
pub fn freeze_handin_stage(
    input: NpcHandInStageInput<'_>,
) -> Result<PendingNpcHandIn, InventoryFreezeError> {
    let ticket = input.ticket;
    if input.binding.invocation != ticket.request.event
        || input.binding.source != ticket.request.source.0
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || ticket.inventory.operation == 0
        || !matches!(ticket.category, 1 | 6)
        || (ticket.accepted_count == 0
            && (ticket.category != 1 || !ticket.inventory.proposal.changes.is_empty()))
        || (ticket.accepted_count == 0
            && !ticket
                .inventory
                .proposal
                .participants
                .iter()
                .any(|(item, revision)| {
                    *item == ticket.request.item
                        && input.inventory.items.iter().any(|saved| {
                            saved.entity.object_id == item.0
                                && saved.entity.mutation_revision == *revision
                        })
                }))
        || ticket.accepted_count > ticket.request.count
        || ticket.inventory.actor != ticket.request.context.actor
        || input.inventory.proposal != &ticket.inventory.proposal
        || (ticket.accepted_count > 0
            && !ticket.inventory.proposal.changes.iter().any(|c| {
                c.after.id == ticket.request.item
                    && c.before.as_ref().is_some_and(|before| {
                        before.stack.checked_sub(c.after.stack) == Some(ticket.accepted_count)
                    })
            }))
        || ticket
            .inventory
            .proposal
            .changes
            .iter()
            .any(|c| c.before.is_none())
        || !ticket.checkpoint.pending.is_empty()
        || !ticket.checkpoint.vm.pending.is_empty()
        || !ticket.checkpoint.vm.detached.is_empty()
        || ticket.checkpoint.vm.work.len() > 1
        || ticket.checkpoint.active_operation != ticket.request.operation
        || ticket.checkpoint.vm.work.first().is_some_and(|row| {
            row.context.source != ticket.request.source
                || row.context.target != Some(ticket.request.context.actor)
        })
        || !input.inventory.leases.iter().any(|l| {
            l.character_id == ticket.request.context.actor.0
                && l.state == OwnershipState::Online
                && l.epoch > 0
        })
    {
        return Err(InventoryFreezeError::Identity);
    }
    let checkpoint = freeze_checkpoint(input.binding, 0, ticket.checkpoint.clone())?;
    let operation_id = stage_id(input.binding, 0);
    let mut inventory = freeze_inventory(InventoryFreezeInput {
        operation_id: &operation_id,
        ..input.inventory
    })?;
    inventory.participants.push(ticket.request.source.0);
    inventory.participants.sort_unstable();
    inventory.participants.dedup();
    if inventory.participants.len() > 1024 {
        return Err(InventoryFreezeError::Capacity);
    }
    Ok(PendingNpcHandIn {
        operation: NpcStageOperation {
            inventory,
            workflow: NpcWorkflowUpdate {
                invocation: input.binding.invocation,
                world_epoch: input.world_epoch,
                expected_version: 0,
                checkpoint: checkpoint.encode()?,
            },
        },
        ticket: ticket.clone(),
        receiver: None,
        uncertain: false,
        terminal: false,
    })
}
