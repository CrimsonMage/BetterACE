//! Correlated first-save acquisition. A private reply channel binds a durable
//! world epoch, frozen request, inventory ticket and transient descendant set.
use crate::game_inventory::{
    InventoryFreezeError, InventoryFreezeInput, freeze_generated_inventory,
};
use crate::saves::{SaveFailure, SaveHandle, SaveSubmitError, SaveTicket, WriteOutcome};
use bace_persistence::{OperationOutcome, SaveAck, WorldPlacementOperation};
use bace_simulation::{InventoryReceipt, InventoryTicket};
use bace_types::EntityId;

pub enum GeneratedSaveResolution {
    Committed {
        receipt: InventoryReceipt,
        transient: Vec<EntityId>,
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
pub struct PendingGeneratedSave {
    operation: WorldPlacementOperation,
    creature_roots: Vec<u32>,
    ticket: InventoryTicket,
    transient: Vec<EntityId>,
    receiver: Option<SaveTicket>,
    terminal: bool,
    uncertain: bool,
}
impl PendingGeneratedSave {
    pub fn freeze(
        ticket: InventoryTicket,
        transient: &[EntityId],
        input: InventoryFreezeInput<'_>,
        world_epoch: u64,
    ) -> Result<Self, InventoryFreezeError> {
        if ticket.proposal != *input.proposal
            || !input.leases.iter().any(|lease| {
                lease.character_id == ticket.actor.0
                    && lease.state == bace_persistence::OwnershipState::Online
            })
        {
            return Err(InventoryFreezeError::Identity);
        }
        let ids: Vec<_> = transient.iter().map(|id| id.0).collect();
        let transient_set: std::collections::BTreeSet<_> = ids.iter().copied().collect();
        let mut creature_roots: Vec<_> = input
            .items
            .iter()
            .filter(|item| {
                transient_set.contains(&item.entity.object_id)
                    && item.construction.is_some()
                    && matches!(item.entity.state.weenie_type, 10 | 15)
            })
            .map(|item| item.entity.object_id)
            .collect();
        creature_roots.sort_unstable();
        creature_roots.dedup();
        if creature_roots.len() > 128 {
            return Err(InventoryFreezeError::Capacity);
        }
        let operation = freeze_generated_inventory(input, &ids, world_epoch)?;
        Ok(Self {
            operation,
            creature_roots,
            ticket,
            transient: transient.to_vec(),
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
        self.receiver = Some(if self.creature_roots.is_empty() {
            saves.try_world_placement(&self.operation)?
        } else {
            saves.try_constructed_creature_promotion(
                &bace_persistence::ConstructedCreaturePromotionOperation {
                    world_epoch: self.operation.world_epoch,
                    inventory: self.operation.inventory.clone(),
                    creature_roots: self.creature_roots.clone(),
                },
            )?
        });
        Ok(())
    }
    pub fn poll(&mut self) -> Option<GeneratedSaveResolution> {
        let receiver = self.receiver.as_mut()?;
        let report = match receiver.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.receiver = None;
                return Some(self.unresolved("generated acquisition reply closed".into()));
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
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(mut acks))) => {
                acks.sort_by_key(|ack| ack.object_id);
                if acks != expected {
                    return Some(self.unresolved("generated acquisition receipt mismatch".into()));
                }
            }
            Err(
                failure @ SaveFailure::Storage {
                    uncertain: false, ..
                },
            ) if !self.uncertain => {
                self.terminal = true;
                return Some(GeneratedSaveResolution::Rejected {
                    operation: self.ticket.operation,
                    failure,
                });
            }
            Err(failure) => return Some(self.unresolved(failure.to_string())),
            Ok(_) => {
                return Some(self.unresolved("unexpected generated acquisition outcome".into()));
            }
        }
        self.terminal = true;
        Some(GeneratedSaveResolution::Committed {
            receipt: InventoryReceipt {
                operation: self.ticket.operation,
                revisions: self
                    .ticket
                    .proposal
                    .changes
                    .iter()
                    .map(|c| (c.after.id, c.after.revision))
                    .collect(),
            },
            transient: self.transient.clone(),
            acknowledgments: expected,
        })
    }
    fn unresolved(&mut self, message: String) -> GeneratedSaveResolution {
        self.uncertain = true;
        GeneratedSaveResolution::Uncertain { message }
    }
}
