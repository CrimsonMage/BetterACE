//! Burned components and the exact reserved mana debit share one durable write.
//! Recovery is captured before the effect; no future cast success is invented.
use crate::game_inventory::{InventoryFreezeError, InventoryFreezeInput, freeze_inventory};
use crate::saves::{
    SaveFailure, SaveHandle, SaveReport, SaveSubmitError, SaveTicket, WriteOutcome,
};
use bace_persistence::{
    CharacterLease, OperationOutcome, OwnershipState, PlacementOperation, SaveAck, SaveSnapshot,
};
use bace_simulation::{InventoryReceipt, InventoryTicket, MagicResourceCommit};
use bace_storage_codec::{CombatRecoverySaveV1, PlayerSaveV6, SaveCodecError};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MagicOperationId([u8; 16]);
impl MagicOperationId {
    pub fn new(id: [u8; 16]) -> Result<Self, MagicSaveError> {
        if id == [0; 16] {
            Err(MagicSaveError::Identity)
        } else {
            Ok(Self(id))
        }
    }
    fn durable(self) -> String {
        use std::fmt::Write;
        let mut id = String::from("magic:");
        for byte in self.0 {
            write!(id, "{byte:02x}").expect("String write");
        }
        id
    }
}
#[derive(Debug, thiserror::Error)]
pub enum MagicSaveError {
    #[error("magic inventory/resource identity, revision or capture clock mismatch")]
    Identity,
    #[error(transparent)]
    Inventory(#[from] InventoryFreezeError),
    #[error(transparent)]
    Codec(#[from] SaveCodecError),
}
pub struct MagicSaveInput<'a> {
    pub id: MagicOperationId,
    pub ticket: &'a InventoryTicket,
    pub resources: &'a MagicResourceCommit,
    pub player: &'a PlayerSaveV6,
    pub player_version: i64,
    pub lease: CharacterLease,
    /// Trusted UTC corresponding to resources.prepared_at, not worker completion.
    pub captured_unix_millis: u64,
    pub inventory: InventoryFreezeInput<'a>,
}
#[derive(Debug)]
pub enum MagicSaveResolution {
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
/// Retain this exact frozen operation until its private reply resolves. A later
/// definite failure cannot authorize rollback after an uncertain commit.
pub struct PendingMagicSave {
    operation: PlacementOperation,
    receipt: InventoryReceipt,
    receiver: Option<SaveTicket>,
    terminal: bool,
    uncertain: bool,
}
impl PendingMagicSave {
    pub fn operation(&self) -> &PlacementOperation {
        &self.operation
    }
    pub fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        if self.terminal || self.receiver.is_some() {
            return Err(SaveSubmitError::Invalid);
        }
        self.receiver = Some(saves.try_placement(&self.operation)?);
        Ok(())
    }
    pub fn poll(&mut self) -> Option<MagicSaveResolution> {
        let receiver = self.receiver.as_mut()?;
        let report = match receiver.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.receiver = None;
                self.uncertain = true;
                return Some(MagicSaveResolution::Uncertain {
                    message: "magic save reply closed; durable outcome unresolved".into(),
                });
            }
        };
        self.receiver = None;
        let result = self.resolve(report);
        if matches!(result, MagicSaveResolution::Uncertain { .. }) {
            self.uncertain = true;
        }
        Some(result)
    }
    fn resolve(&mut self, report: SaveReport) -> MagicSaveResolution {
        let expected: Vec<_> = self
            .operation
            .snapshots
            .iter()
            .map(|s| SaveAck {
                object_id: s.object_id,
                mutation_revision: s.mutation_revision,
                persisted_version: s.expected_version + 1,
            })
            .collect();
        let acknowledgments = match report.result {
            Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)) => expected,
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(mut acks))) => {
                let mut expected = expected;
                expected.sort_by_key(|a| a.object_id);
                acks.sort_by_key(|a| a.object_id);
                if acks != expected {
                    return MagicSaveResolution::Uncertain {
                        message: "magic save acknowledgment does not match all frozen participants"
                            .into(),
                    };
                }
                acks
            }
            Err(
                failure @ SaveFailure::Storage {
                    uncertain: false, ..
                },
            ) if !self.uncertain => {
                self.terminal = true;
                return MagicSaveResolution::Rejected {
                    operation: self.receipt.operation,
                    failure,
                };
            }
            Err(failure) => {
                return MagicSaveResolution::Uncertain {
                    message: failure.to_string(),
                };
            }
            Ok(_) => {
                return MagicSaveResolution::Uncertain {
                    message: "unexpected magic save result".into(),
                };
            }
        };
        self.terminal = true;
        MagicSaveResolution::Committed {
            receipt: self.receipt.clone(),
            acknowledgments,
        }
    }
}
pub fn freeze_magic_inventory(
    input: MagicSaveInput<'_>,
) -> Result<PendingMagicSave, MagicSaveError> {
    let resources = input.resources;
    if input.ticket.operation == 0
        || resources.operation != input.ticket.operation
        || resources.cast == 0
        || input.ticket.actor != resources.actor
        || resources.actor.0 != input.player.player.entity.object_id
        || resources.mana.actor != resources.actor
        || resources.mana.vital != bace_entity::EntityVital::Mana
        || resources.mana.after > resources.mana.before
        || !resources.prepared_at.is_finite()
        || resources.prepared_at < 0.0
        || input.player_version <= 0
        || input.player_version == i64::MAX
        || resources.before_revision != input.player.player.entity.mutation_revision
        || resources.before_revision.checked_add(1) != Some(resources.after_revision)
        || input.lease.character_id != resources.actor.0
        || input.lease.epoch <= 0
        || input.lease.state != OwnershipState::Online
        || input.inventory.leases != [input.lease]
        || input.inventory.proposal != &input.ticket.proposal
        || input.ticket.proposal.changes.is_empty()
    {
        return Err(MagicSaveError::Identity);
    }
    input.player.validate()?;
    let mut player = input.player.clone();
    let mana = player
        .player
        .entity
        .state
        .properties
        .secondary_attributes
        .iter_mut()
        .find(|v| v.id == 5)
        .ok_or(MagicSaveError::Identity)?;
    if mana.value.current_level != resources.mana.before {
        return Err(MagicSaveError::Identity);
    }
    mana.value.current_level = resources.mana.after;
    let recovery = CombatRecoverySaveV1 {
        captured_unix_millis: input.captured_unix_millis,
        state: crate::magic_recovery::freeze_cast_recovery(resources.recovery)?,
    };
    bace_storage_codec::validate_recovery_transition(player.combat_recovery, Some(recovery))?;
    player.combat_recovery = Some(recovery);
    player.player.entity.mutation_revision = resources.after_revision;
    let snapshot = SaveSnapshot {
        object_id: resources.actor.0,
        mutation_revision: resources.after_revision,
        expected_version: input.player_version,
        bytes: player.encode()?,
    };
    if let Some(prior) = input
        .inventory
        .other_snapshots
        .iter()
        .find(|s| s.object_id == resources.actor.0)
        && (prior.expected_version != input.player_version
            || prior.mutation_revision != resources.before_revision
            || prior.bytes != input.player.encode()?)
    {
        return Err(MagicSaveError::Identity);
    }
    let id = input.id.durable();
    let mut operation = freeze_inventory(InventoryFreezeInput {
        operation_id: &id,
        ..input.inventory
    })?;
    operation
        .snapshots
        .retain(|s| s.object_id != snapshot.object_id);
    operation.snapshots.push(snapshot);
    if operation.snapshots.len() > 1024
        || operation
            .snapshots
            .iter()
            .any(|s| s.expected_version < 0 || s.expected_version == i64::MAX)
    {
        return Err(MagicSaveError::Identity);
    }
    let receipt = InventoryReceipt {
        operation: input.ticket.operation,
        revisions: input
            .ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    Ok(PendingMagicSave {
        operation,
        receipt,
        receiver: None,
        terminal: false,
        uncertain: false,
    })
}

#[cfg(test)]
mod tests;
