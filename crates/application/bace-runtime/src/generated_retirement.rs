//! Epoch-fenced generated tombstones. A private reply channel binds the exact
//! lifecycle effect, proposal, before revisions and transient descendant set.
use crate::game_inventory::{
    InventoryFreezeError, InventoryFreezeInput, freeze_generated_inventory, freeze_inventory,
};
use crate::saves::{SaveFailure, SaveHandle, SaveSubmitError, SaveTicket, WriteOutcome};
use bace_persistence::{OperationOutcome, SaveAck, WorldPlacementOperation};
use bace_simulation::{GeneratedRetirementTicket, InventoryReceipt};

pub enum GeneratedRetirementResolution {
    Committed {
        receipt: InventoryReceipt,
        acknowledgments: Vec<SaveAck>,
    },
    Rejected {
        operation: u64,
        failure: SaveFailure,
    },
    Uncertain {
        message: String,
    },
}
pub struct PendingGeneratedRetirement {
    operation: WorldPlacementOperation,
    ticket: GeneratedRetirementTicket,
    receiver: Option<SaveTicket>,
    terminal: bool,
    uncertain: bool,
}
impl PendingGeneratedRetirement {
    pub fn freeze(
        ticket: GeneratedRetirementTicket,
        input: InventoryFreezeInput<'_>,
        world_epoch: u64,
    ) -> Result<Self, InventoryFreezeError> {
        if ticket.npc_source_ticket.is_some() {
            return Err(InventoryFreezeError::Identity);
        }
        let operation = freeze_retirement_operation(&ticket, input, world_epoch)?;
        Ok(Self {
            operation,
            ticket,
            receiver: None,
            terminal: false,
            uncertain: false,
        })
    }
    pub fn operation(&self) -> &WorldPlacementOperation {
        &self.operation
    }
    pub fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        if self.terminal || self.receiver.is_some() {
            return Err(SaveSubmitError::Invalid);
        }
        self.receiver = Some(saves.try_world_placement(&self.operation)?);
        Ok(())
    }
    pub fn poll(&mut self) -> Option<GeneratedRetirementResolution> {
        let receiver = self.receiver.as_mut()?;
        let report = match receiver.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.receiver = None;
                return Some(self.unresolved("generated retirement reply closed".into()));
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
        expected.sort_by_key(|ack| ack.object_id);
        match report.result {
            Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)) => {}
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(mut acknowledgments))) => {
                acknowledgments.sort_by_key(|ack| ack.object_id);
                if acknowledgments != expected {
                    return Some(self.unresolved("generated retirement receipt mismatch".into()));
                }
            }
            Err(
                failure @ SaveFailure::Storage {
                    uncertain: false, ..
                },
            ) if !self.uncertain => {
                self.terminal = true;
                return Some(GeneratedRetirementResolution::Rejected {
                    operation: self.ticket.inventory.operation,
                    failure,
                });
            }
            Err(failure) => return Some(self.unresolved(failure.to_string())),
            Ok(_) => {
                return Some(self.unresolved("unexpected generated retirement outcome".into()));
            }
        }
        self.terminal = true;
        Some(GeneratedRetirementResolution::Committed {
            receipt: InventoryReceipt {
                operation: self.ticket.inventory.operation,
                revisions: self
                    .ticket
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .map(|change| (change.after.id, change.after.revision))
                    .collect(),
            },
            acknowledgments: expected,
        })
    }
    fn unresolved(&mut self, message: String) -> GeneratedRetirementResolution {
        self.uncertain = true;
        GeneratedRetirementResolution::Uncertain { message }
    }
}

/// NPC callers receive only immutable operation bytes, never a standalone save
/// controller that could omit the source workflow from its transaction.
pub(crate) fn freeze_npc_retirement_operation(
    ticket: &GeneratedRetirementTicket,
    input: InventoryFreezeInput<'_>,
    world_epoch: u64,
    npc_ticket: u64,
) -> Result<WorldPlacementOperation, InventoryFreezeError> {
    if npc_ticket == 0 || ticket.npc_source_ticket != Some(npc_ticket) {
        return Err(InventoryFreezeError::Identity);
    }
    freeze_retirement_operation(ticket, input, world_epoch)
}
fn freeze_retirement_operation(
    ticket: &GeneratedRetirementTicket,
    input: InventoryFreezeInput<'_>,
    world_epoch: u64,
) -> Result<WorldPlacementOperation, InventoryFreezeError> {
    if ticket.inventory.proposal != *input.proposal
        || world_epoch == 0
        || world_epoch > i64::MAX as u64
        || !input.other_snapshots.is_empty()
        || !ticket
            .inventory
            .proposal
            .changes
            .iter()
            .any(|c| c.after.place == bace_inventory::ItemPlace::Removed)
    {
        return Err(InventoryFreezeError::Identity);
    }
    if ticket.transient.is_empty() {
        Ok(WorldPlacementOperation {
            world_epoch,
            inventory: freeze_inventory(input)?,
        })
    } else {
        let ids: Vec<_> = ticket.transient.iter().map(|id| id.0).collect();
        freeze_generated_inventory(input, &ids, world_epoch)
    }
}
