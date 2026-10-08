//! Correlated durable skill writes. The owner retains reservations until a
//! matching success or definite rejection; uncertainty retries identical bytes.
use crate::game_inventory::{InventoryFreezeError, InventoryFreezeInput, freeze_inventory};
use crate::progression_saves::{ProgressionSaveError, freeze_skill_change};
use crate::saves::{
    SaveFailure, SaveHandle, SaveReport, SaveSubmitError, SaveTicket, WriteOutcome,
};
use bace_persistence::{
    CharacterLease, OperationOutcome, OwnershipState, PlacementOperation, SaveAck, SaveSnapshot,
};
use bace_simulation::{InventoryReceipt, SkillDeviceTicket, SkillTicket};
use bace_storage_codec::PlayerSaveV6;
mod attribute_transfer;

/// Caller allocates this durable identity once, independently of process-local
/// ticket counters, and retains it across retries and uncertain completions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillOperationId([u8; 16]);
impl SkillOperationId {
    pub fn new(value: [u8; 16]) -> Result<Self, SkillSaveError> {
        if value == [0; 16] {
            return Err(SkillSaveError::Identity);
        }
        Ok(Self(value))
    }
    fn durable(self) -> String {
        use std::fmt::Write;
        let mut result = String::from("skill:");
        for byte in self.0 {
            write!(result, "{byte:02x}").expect("String write");
        }
        result
    }
    fn durable_attribute_transfer(self) -> String {
        self.durable().replacen("skill:", "attribute-transfer:", 1)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum SkillSaveError {
    #[error("skill save identity, lease or revision mismatch")]
    Identity,
    #[error("skill snapshot rejected: {0:?}")]
    Progression(ProgressionSaveError),
    #[error(transparent)]
    Inventory(#[from] InventoryFreezeError),
    #[error(transparent)]
    Codec(#[from] bace_storage_codec::SaveCodecError),
}
#[derive(Clone, Debug, PartialEq)]
pub enum SkillSaveOwner {
    Plain(SkillTicket),
    Device(SkillDeviceTicket),
    AttributeTransfer(bace_simulation::AttributeTransferDeviceTicket),
}
#[derive(Debug)]
pub enum SkillSaveResolution {
    Committed {
        owner: SkillSaveOwner,
        acknowledgments: Vec<SaveAck>,
        inventory: Option<InventoryReceipt>,
    },
    /// Definite transaction rejection permits rolling back the matching owner.
    Rejected {
        owner: SkillSaveOwner,
        failure: SaveFailure,
    },
    /// Keep every reservation. submit() resolves by retrying this same operation;
    /// the storage journal compares the complete request fingerprint first.
    Uncertain { message: String },
}
/// A single bounded operation, holding the exact frozen snapshot and its own
/// oneshot receiver. Arbitrary/unrelated SaveReports cannot be injected.
pub struct PendingSkillSave {
    operation: PlacementOperation,
    owner: SkillSaveOwner,
    receiver: Option<SaveTicket>,
    terminal: bool,
    uncertain: bool,
}
impl PendingSkillSave {
    /// Join pre-existing dirty inventory captures before any submission. Rows
    /// already modified by the skill device keep that operation's after-state.
    pub(crate) fn join_captured_inventory(
        &mut self,
        rows: Vec<SaveSnapshot>,
    ) -> Result<(), SkillSaveError> {
        if self.receiver.is_some() || self.terminal || self.uncertain {
            return Err(SkillSaveError::Identity);
        }
        let mut snapshots = self.operation.snapshots.clone();
        let mut participants: std::collections::BTreeSet<_> =
            self.operation.participants.iter().copied().collect();
        for row in rows {
            if snapshots.iter().any(|s| s.object_id == row.object_id) {
                continue;
            }
            let item = bace_storage_codec::ItemSaveV5::decode(&row.bytes)?;
            if item.entity.object_id != row.object_id
                || item.entity.mutation_revision != row.mutation_revision
                || row.expected_version <= 0
                || row.expected_version == i64::MAX
            {
                return Err(SkillSaveError::Identity);
            }
            participants.insert(row.object_id);
            snapshots.push(row);
        }
        if snapshots.len() > 1024
            || participants.len() > 1024
            || snapshots.iter().map(|s| s.bytes.len()).sum::<usize>() > 64 * 1024 * 1024
        {
            return Err(SkillSaveError::Identity);
        }
        snapshots.sort_by_key(|s| s.object_id);
        self.operation.snapshots = snapshots;
        self.operation.participants = participants.into_iter().collect();
        Ok(())
    }
    pub fn operation(&self) -> &PlacementOperation {
        &self.operation
    }
    pub fn owner(&self) -> &SkillSaveOwner {
        &self.owner
    }
    pub fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        if self.terminal || self.receiver.is_some() {
            return Err(SaveSubmitError::Invalid);
        }
        self.receiver = Some(saves.try_placement(&self.operation)?);
        Ok(())
    }
    /// Nonblocking, for the runtime owner loop. A closed reply is uncertain.
    pub fn poll(&mut self) -> Option<SkillSaveResolution> {
        let receiver = self.receiver.as_mut()?;
        let report = match receiver.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.receiver = None;
                self.uncertain = true;
                return Some(SkillSaveResolution::Uncertain {
                    message: "save reply closed; durable outcome unresolved".into(),
                });
            }
        };
        self.receiver = None;
        let resolution = self.resolve(report);
        if matches!(resolution, SkillSaveResolution::Uncertain { .. }) {
            self.uncertain = true;
        }
        Some(resolution)
    }
    fn resolve(&mut self, report: SaveReport) -> SkillSaveResolution {
        let expected: Vec<_> = self
            .operation
            .snapshots
            .iter()
            .map(|snapshot| SaveAck {
                object_id: snapshot.object_id,
                mutation_revision: snapshot.mutation_revision,
                persisted_version: snapshot.expected_version + 1,
            })
            .collect();
        let result = match report.result {
            Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)) => expected.clone(),
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(mut acks))) => {
                let mut check = expected;
                check.sort_by_key(|ack| ack.object_id);
                acks.sort_by_key(|ack| ack.object_id);
                if acks != check {
                    return SkillSaveResolution::Uncertain {
                        message: "save acknowledgment does not match frozen skill operation".into(),
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
                return SkillSaveResolution::Rejected {
                    owner: self.owner.clone(),
                    failure,
                };
            }
            Err(failure) => {
                return SkillSaveResolution::Uncertain {
                    message: failure.to_string(),
                };
            }
            Ok(_) => {
                return SkillSaveResolution::Uncertain {
                    message: "unexpected save outcome for skill operation".into(),
                };
            }
        };
        self.terminal = true;
        let inventory = match &self.owner {
            SkillSaveOwner::Plain(_) => None,
            SkillSaveOwner::Device(ticket) => Some(inventory_receipt(&ticket.inventory)),
            SkillSaveOwner::AttributeTransfer(ticket) => Some(inventory_receipt(&ticket.inventory)),
        };
        SkillSaveResolution::Committed {
            owner: self.owner.clone(),
            acknowledgments: result,
            inventory,
        }
    }
}
fn inventory_receipt(ticket: &bace_simulation::InventoryTicket) -> InventoryReceipt {
    InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|change| (change.after.id, change.after.revision))
            .collect(),
    }
}
fn snapshot(
    saved: &PlayerSaveV6,
    version: i64,
    lease: CharacterLease,
    ticket: SkillTicket,
) -> Result<SaveSnapshot, SkillSaveError> {
    if ticket.operation == 0
        || ticket.context.actor.0 != saved.player.entity.object_id
        || ticket.context.account.0 != saved.player.account_id
        || lease.character_id != ticket.context.actor.0
        || lease.epoch <= 0
        || lease.state != OwnershipState::Online
        || version <= 0
        || version == i64::MAX
    {
        return Err(SkillSaveError::Identity);
    }
    let next = freeze_skill_change(saved, ticket.change, ticket.expected_revision)
        .map_err(SkillSaveError::Progression)?;
    Ok(SaveSnapshot {
        object_id: ticket.context.actor.0,
        mutation_revision: ticket.change.revision,
        expected_version: version,
        bytes: next.encode()?,
    })
}
pub fn freeze_skill_ticket(
    id: SkillOperationId,
    ticket: SkillTicket,
    saved: &PlayerSaveV6,
    version: i64,
    lease: CharacterLease,
) -> Result<PendingSkillSave, SkillSaveError> {
    let snapshot = snapshot(saved, version, lease, ticket)?;
    Ok(PendingSkillSave {
        operation: PlacementOperation {
            operation_id: id.durable(),
            snapshots: vec![snapshot],
            participants: vec![ticket.context.actor.0],
            leases: vec![lease],
            changes: vec![],
            storage_views: vec![],
        },
        owner: SkillSaveOwner::Plain(ticket),
        receiver: None,
        terminal: false,
        uncertain: false,
    })
}
/// The inventory input must be the exact reserved composite proposal. Its
/// operation_id is replaced by the durable caller identity, never a local counter.
pub fn freeze_skill_device(
    id: SkillOperationId,
    ticket: SkillDeviceTicket,
    saved: &PlayerSaveV6,
    version: i64,
    lease: CharacterLease,
    inventory: InventoryFreezeInput<'_>,
) -> Result<PendingSkillSave, SkillSaveError> {
    let mut player = snapshot(saved, version, lease, ticket.skill)?;
    if let Some(cooldown) = &ticket.cooldown {
        if cooldown.after_revision
            != cooldown
                .before_revision
                .checked_add(1)
                .ok_or(SkillSaveError::Identity)?
            || cooldown.after.len() != saved.enchantments.len() + 1
            || cooldown.after.last().is_none_or(|entry| {
                entry.spell != u32::from(0x8000 | cooldown.group)
                    || entry.spec.duration != cooldown.seconds
                    || entry.spec.layer != 1
                    || entry.spec.category != 0x8000
                    || entry.caster
                        != ticket
                            .inventory
                            .proposal
                            .changes
                            .first()
                            .map_or(0, |change| change.after.id.0)
            })
        {
            return Err(SkillSaveError::Identity);
        }
        let before = cooldown.after[..cooldown.after.len() - 1]
            .iter()
            .map(crate::enchantment_saves::freeze_enchantment)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| SkillSaveError::Identity)?;
        if before != saved.enchantments {
            return Err(SkillSaveError::Identity);
        }
        let mut next = PlayerSaveV6::decode(&player.bytes)?;
        next.enchantments = cooldown
            .after
            .iter()
            .map(crate::enchantment_saves::freeze_enchantment)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| SkillSaveError::Identity)?;
        next.validate()?;
        player.bytes = next.encode()?;
    }
    if inventory.proposal != &ticket.inventory.proposal
        || ticket.inventory.actor != ticket.skill.context.actor
        || ticket.inventory.operation == 0
        || inventory.leases != [lease]
    {
        return Err(SkillSaveError::Identity);
    }
    // Any existing player snapshot must describe exactly the same source owner.
    if let Some(prior) = inventory
        .other_snapshots
        .iter()
        .find(|s| s.object_id == player.object_id)
        && (prior.expected_version != version
            || prior.mutation_revision != ticket.skill.expected_revision
            || prior.bytes != saved.encode()?)
    {
        return Err(SkillSaveError::Identity);
    }
    let operation_id = id.durable();
    let mut operation = freeze_inventory(InventoryFreezeInput {
        operation_id: &operation_id,
        ..inventory
    })?;
    operation
        .snapshots
        .retain(|s| s.object_id != player.object_id);
    operation.snapshots.push(player);
    if operation.snapshots.len() > 1024
        || operation
            .snapshots
            .iter()
            .any(|s| s.expected_version < 0 || s.expected_version == i64::MAX)
    {
        return Err(SkillSaveError::Identity);
    }
    Ok(PendingSkillSave {
        operation,
        owner: SkillSaveOwner::Device(ticket),
        receiver: None,
        terminal: false,
        uncertain: false,
    })
}

/// The character's two attribute records and the consumed inventory item are
/// frozen under one placement operation and one idempotency identity.
pub fn freeze_attribute_transfer_device(
    id: SkillOperationId,
    ticket: bace_simulation::AttributeTransferDeviceTicket,
    saved: &PlayerSaveV6,
    version: i64,
    lease: CharacterLease,
    inventory: InventoryFreezeInput<'_>,
) -> Result<PendingSkillSave, SkillSaveError> {
    let character = ticket.character;
    if character.operation == 0
        || character.context.actor.0 != saved.player.entity.object_id
        || character.context.account.0 != saved.player.account_id
        || lease.character_id != character.context.actor.0
        || lease.epoch <= 0
        || lease.state != OwnershipState::Online
        || version <= 0
        || version == i64::MAX
        || inventory.proposal != &ticket.inventory.proposal
        || ticket.inventory.actor != character.context.actor
        || ticket.inventory.operation == 0
        || inventory.leases != [lease]
    {
        return Err(SkillSaveError::Identity);
    }
    if let Some(prior) = inventory
        .other_snapshots
        .iter()
        .find(|snapshot| snapshot.object_id == character.context.actor.0)
        && (prior.expected_version != version
            || prior.mutation_revision != character.proposal.expected_revision
            || prior.bytes != saved.encode()?)
    {
        return Err(SkillSaveError::Identity);
    }
    let next = attribute_transfer::freeze(saved, character.proposal)?;
    let player = SaveSnapshot {
        object_id: character.context.actor.0,
        mutation_revision: character.proposal.revision,
        expected_version: version,
        bytes: next.encode()?,
    };
    let operation_id = id.durable_attribute_transfer();
    let mut operation = freeze_inventory(InventoryFreezeInput {
        operation_id: &operation_id,
        ..inventory
    })?;
    operation
        .snapshots
        .retain(|snapshot| snapshot.object_id != player.object_id);
    operation.snapshots.push(player);
    if operation.snapshots.len() > 1024
        || operation
            .snapshots
            .iter()
            .any(|snapshot| snapshot.expected_version < 0 || snapshot.expected_version == i64::MAX)
    {
        return Err(SkillSaveError::Identity);
    }
    Ok(PendingSkillSave {
        operation,
        owner: SkillSaveOwner::AttributeTransfer(ticket),
        receiver: None,
        terminal: false,
        uncertain: false,
    })
}
