//! Retained runtime lifecycle for skill training and consumable skill devices.
//! One operation at a time bounds captures and serializes uncorrelated device
//! commit replies. Gameplay reservations remain in the simulation owner.
use crate::{
    online_player_saves::OnlinePlayerSaveService,
    saves::{SaveHandle, SaveSubmitError},
    simulation::SimulationInput,
    skill_saves::{PendingSkillSave, SkillOperationId, SkillSaveOwner, SkillSaveResolution},
};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::SaveSnapshot;
use bace_simulation::{
    AttributeTransferCommand, AttributeTransferOutcome, AttributeTransferResult, Command,
    PlayerReadSnapshot, PlayerSnapshotOperation, PlayerSnapshotOutcome, PlayerSnapshotRequest,
    SkillDeviceCommand, SkillDeviceOutcome, SkillDeviceResult, SkillOutcome, SkillStage,
};
use std::sync::{Arc, mpsc::TrySendError};
mod freezing;

#[derive(Clone, Debug, PartialEq)]
pub struct SkillCompletion {
    pub owner: SkillSaveOwner,
    pub committed: bool,
}
enum Phase {
    Barrier,
    CaptureReady,
    Capturing(u64),
    Captured(Arc<PlayerReadSnapshot>, u64),
    Saving {
        save: Box<PendingSkillSave>,
        submitted: bool,
    },
    Delivering {
        command: Option<Box<Command>>,
        committed: Option<Vec<SaveSnapshot>>,
    },
    Finishing {
        committed: Option<Vec<SaveSnapshot>>,
    },
}
struct Pending {
    identity: SkillOperationId,
    owner: SkillSaveOwner,
    phase: Phase,
    baseline_reserved: bool,
}
/// Retain the service on shutdown while `requires_drain()` is true. Queue
/// admission never releases a baseline or reports a successful skill change.
pub struct SkillService {
    pending: Option<Pending>,
    completion: Option<SkillCompletion>,
    blocked: Option<String>,
    last_correlation: u64,
}
impl Default for SkillService {
    fn default() -> Self {
        Self::new()
    }
}
impl SkillService {
    pub fn new() -> Self {
        Self {
            pending: None,
            completion: None,
            blocked: None,
            last_correlation: 0,
        }
    }
    /// Accept only a proposal returned by the authoritative skill owner. Allocate
    /// the durable random identity once; it survives all subsequent retries.
    pub fn stage(
        &mut self,
        identity: SkillOperationId,
        owner: SkillSaveOwner,
    ) -> Result<(), Box<SkillSaveOwner>> {
        if self.requires_drain() {
            return Err(Box::new(owner));
        }
        self.blocked = None;
        self.pending = Some(Pending {
            identity,
            owner,
            phase: Phase::Barrier,
            baseline_reserved: false,
        });
        Ok(())
    }
    pub fn requires_drain(&self) -> bool {
        self.pending.is_some() || self.completion.is_some()
    }
    pub fn blocked(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    /// Explicit retry preserves the original operation and any uncertain receipt.
    pub fn retry(&mut self) {
        if self.blocked.is_some()
            && let Some(p) = &mut self.pending
            && let Phase::Delivering { command, committed } = &mut p.phase
            && command.is_none()
        {
            *command = Some(Box::new(if committed.is_some() {
                commit(&p.owner)
            } else {
                rollback(&p.owner)
            }));
        }
        self.blocked = None;
    }
    pub fn take_completion(&mut self) -> Option<SkillCompletion> {
        self.completion.take()
    }
    /// Definite preparation failure may roll back before a write is submitted.
    /// Captures in flight must return first; saving/uncertain work cannot cancel.
    pub fn reject_unsubmitted(&mut self) -> Result<(), String> {
        let p = self.pending.as_mut().ok_or("no pending skill")?;
        if !matches!(
            p.phase,
            Phase::Barrier | Phase::CaptureReady | Phase::Captured(_, _)
        ) {
            return Err("skill capture or durable outcome must resolve before cancellation".into());
        }
        p.phase = Phase::Delivering {
            command: Some(Box::new(rollback(&p.owner))),
            committed: None,
        };
        self.blocked = None;
        Ok(())
    }
    pub fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| matches!(p.phase, Phase::Capturing(c) if c == outcome.correlation))
    }
    pub fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix_millis: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if !self.owns_capture(&outcome) {
            return Err(outcome);
        }
        let p = self.pending.as_mut().expect("matched capture");
        match outcome.result {
            Ok(snapshot) => p.phase = Phase::Captured(snapshot, unix_millis),
            Err(error) => {
                p.phase = Phase::CaptureReady;
                self.blocked = Some(format!("skill capture rejected: {error:?}"));
            }
        }
        Ok(())
    }
    /// Bounded one-stage work. Correlations come from the application's shared
    /// snapshot allocator, also used by routine saves and other critical services.
    pub fn poll(
        &mut self,
        input: &SimulationInput,
        online: &mut OnlinePlayerSaveService,
        saves: &SaveHandle,
        correlation: u64,
    ) -> Result<(), String> {
        self.poll_with(online, saves, correlation, |command| {
            input.try_submit(command).map_err(Box::new)
        })
    }
    fn poll_with(
        &mut self,
        online: &mut OnlinePlayerSaveService,
        saves: &SaveHandle,
        correlation: u64,
        mut send: impl FnMut(Command) -> Result<(), Box<TrySendError<Command>>>,
    ) -> Result<(), String> {
        if let Some(error) = &self.blocked {
            return Err(error.clone());
        }
        let Some(p) = self.pending.as_mut() else {
            return Ok(());
        };
        let ticket = reference(&p.owner);
        let mut finished = false;
        let result = (|| -> Result<(), String> {
            match &mut p.phase {
                Phase::Barrier => {
                    if !online.critical_ready(&[ticket.context.actor.0])? {
                        return Ok(());
                    }
                    online.begin_critical(&[ticket.context.actor.0])?;
                    p.baseline_reserved = true;
                    p.phase = Phase::CaptureReady;
                }
                Phase::CaptureReady => {
                    if correlation == 0 || correlation <= self.last_correlation {
                        return Err("stale skill snapshot correlation".into());
                    }
                    let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                        correlation,
                        binding: binding(ticket),
                        operation: Some((
                            match &p.owner {
                                SkillSaveOwner::AttributeTransfer(_) => {
                                    PlayerSnapshotOperation::AttributeTransfer(ticket.operation)
                                }
                                _ => PlayerSnapshotOperation::Skill(ticket.operation),
                            },
                            ticket.expected_revision,
                        )),
                    });
                    match send(command) {
                        Ok(()) => {
                            self.last_correlation = correlation;
                            p.phase = Phase::Capturing(correlation);
                        }
                        Err(error) => match *error {
                            TrySendError::Full(_) => {}
                            TrySendError::Disconnected(_) => {
                                return Err("skill snapshot ingress closed".into());
                            }
                        },
                    }
                }
                Phase::Capturing(_) => {}
                Phase::Captured(snapshot, unix_millis) => {
                    let save =
                        freezing::freeze(p.identity, &p.owner, snapshot, *unix_millis, online)?;
                    p.phase = Phase::Saving {
                        save: Box::new(save),
                        submitted: false,
                    };
                }
                Phase::Saving { save, submitted } => {
                    if !*submitted {
                        match save.submit(saves) {
                            Ok(()) => *submitted = true,
                            Err(SaveSubmitError::Full) => {}
                            Err(error) => return Err(error.to_string()),
                        }
                        return Ok(());
                    }
                    let Some(result) = save.poll() else {
                        return Ok(());
                    };
                    *submitted = false;
                    match result {
                        SkillSaveResolution::Uncertain { message } => return Err(message),
                        SkillSaveResolution::Rejected { .. } => {
                            let command = rollback(&p.owner);
                            p.phase = Phase::Delivering {
                                command: Some(Box::new(command)),
                                committed: None,
                            };
                        }
                        SkillSaveResolution::Committed { inventory, .. } => {
                            let mut rows = save.operation().snapshots.clone();
                            for row in &mut rows {
                                row.expected_version = row
                                    .expected_version
                                    .checked_add(1)
                                    .ok_or("skill persisted version overflow")?;
                            }
                            let command = match (&p.owner, inventory) {
                                (SkillSaveOwner::Plain(ticket), None) => {
                                    Command::CommitSkill { ticket: *ticket }
                                }
                                (SkillSaveOwner::Device(_), Some(receipt)) => {
                                    Command::SkillDevice(SkillDeviceCommand::Commit { receipt })
                                }
                                (SkillSaveOwner::AttributeTransfer(_), Some(receipt)) => {
                                    Command::AttributeTransfer(AttributeTransferCommand::Commit {
                                        receipt,
                                    })
                                }
                                _ => return Err("skill receipt owner mismatch".into()),
                            };
                            p.phase = Phase::Delivering {
                                command: Some(Box::new(command)),
                                committed: Some(rows),
                            };
                        }
                    }
                }
                Phase::Delivering { command, .. } => {
                    if let Some(value) = command.take() {
                        match send(*value) {
                            Ok(()) => {}
                            Err(error) => match *error {
                                TrySendError::Full(value) => *command = Some(Box::new(value)),
                                TrySendError::Disconnected(value) => {
                                    *command = Some(Box::new(value));
                                    return Err("skill owner ingress closed".into());
                                }
                            },
                        }
                    }
                }
                Phase::Finishing { committed } => {
                    if let Some(rows) = committed {
                        online.finish_critical(rows)?;
                    } else if p.baseline_reserved {
                        online.cancel_critical(&[ticket.context.actor.0])?;
                    }
                    self.completion = Some(SkillCompletion {
                        owner: p.owner.clone(),
                        committed: committed.is_some(),
                    });
                    finished = true;
                }
            }
            Ok(())
        })();
        if finished {
            self.pending = None;
        }
        if let Err(error) = &result {
            self.blocked = Some(error.clone());
        }
        result
    }
    /// Returns unrelated outcomes intact for projection or their owning service.
    pub fn accept_skill(&mut self, outcome: SkillOutcome) -> Result<(), Box<SkillOutcome>> {
        let Some(p) = self.pending.as_ref() else {
            return Err(Box::new(outcome));
        };
        let SkillSaveOwner::Plain(ticket) = p.owner else {
            return Err(Box::new(outcome));
        };
        if outcome.context != ticket.context || !awaiting_owner(&p.phase) {
            return Err(Box::new(outcome));
        }
        let committed = is_commit(&p.phase);
        match outcome.result {
            Ok(SkillStage::Committed(t)) if committed && t == ticket => self.owner_accepted(),
            Ok(SkillStage::RolledBack(t)) if !committed && t == ticket => self.owner_accepted(),
            Err(error) => {
                self.blocked = Some(format!(
                    "skill owner rejected durable resolution: {error:?}"
                ))
            }
            _ => return Err(Box::new(outcome)),
        }
        Ok(())
    }
    pub fn accept_device(
        &mut self,
        outcome: SkillDeviceOutcome,
    ) -> Result<(), Box<SkillDeviceOutcome>> {
        let Some(p) = self.pending.as_ref() else {
            return Err(Box::new(outcome));
        };
        let SkillSaveOwner::Device(ticket) = &p.owner else {
            return Err(Box::new(outcome));
        };
        if outcome.context.is_some() || !awaiting_owner(&p.phase) {
            return Err(Box::new(outcome));
        }
        let committed = is_commit(&p.phase);
        match &outcome.result {
            Ok(SkillDeviceResult::Committed(t)) if committed && t == ticket => {
                self.owner_accepted()
            }
            Ok(SkillDeviceResult::RolledBack(op))
                if !committed && *op == ticket.inventory.operation =>
            {
                self.owner_accepted()
            }
            Err(error) => {
                self.blocked = Some(format!(
                    "skill device rejected durable resolution: {error:?}"
                ))
            }
            _ => return Err(Box::new(outcome)),
        }
        Ok(())
    }
    pub fn accept_attribute_transfer(
        &mut self,
        outcome: AttributeTransferOutcome,
    ) -> Result<(), Box<AttributeTransferOutcome>> {
        let Some(p) = self.pending.as_ref() else {
            return Err(Box::new(outcome));
        };
        let SkillSaveOwner::AttributeTransfer(ticket) = &p.owner else {
            return Err(Box::new(outcome));
        };
        if outcome.context.is_some() || !awaiting_owner(&p.phase) {
            return Err(Box::new(outcome));
        }
        let committed = is_commit(&p.phase);
        match &outcome.result {
            Ok(AttributeTransferResult::Committed(value)) if committed && value == ticket => {
                self.owner_accepted()
            }
            Ok(AttributeTransferResult::RolledBack(operation))
                if !committed && *operation == ticket.inventory.operation =>
            {
                self.owner_accepted()
            }
            Err(error) => {
                self.blocked = Some(format!(
                    "attribute transfer owner rejected durable resolution: {error:?}"
                ))
            }
            _ => return Err(Box::new(outcome)),
        }
        Ok(())
    }
    fn owner_accepted(&mut self) {
        let p = self.pending.as_mut().expect("matched owner");
        let Phase::Delivering { committed, .. } = &mut p.phase else {
            unreachable!()
        };
        p.phase = Phase::Finishing {
            committed: committed.take(),
        };
    }
}
fn awaiting_owner(phase: &Phase) -> bool {
    matches!(phase, Phase::Delivering { command: None, .. })
}
fn is_commit(phase: &Phase) -> bool {
    matches!(
        phase,
        Phase::Delivering {
            committed: Some(_),
            ..
        }
    )
}
#[derive(Clone, Copy)]
struct OwnerReference {
    context: bace_gameplay_api::ActionContext,
    operation: u64,
    expected_revision: u64,
}
fn reference(owner: &SkillSaveOwner) -> OwnerReference {
    match owner {
        SkillSaveOwner::Plain(ticket) => OwnerReference {
            context: ticket.context,
            operation: ticket.operation,
            expected_revision: ticket.expected_revision,
        },
        SkillSaveOwner::Device(ticket) => OwnerReference {
            context: ticket.skill.context,
            operation: ticket.skill.operation,
            expected_revision: ticket.skill.expected_revision,
        },
        SkillSaveOwner::AttributeTransfer(ticket) => OwnerReference {
            context: ticket.character.context,
            operation: ticket.character.operation,
            expected_revision: ticket.character.proposal.expected_revision,
        },
    }
}
fn binding(ticket: OwnerReference) -> CharacterBinding {
    CharacterBinding {
        actor: ticket.context.actor,
        account: ticket.context.account,
        session: ticket.context.session,
    }
}
fn rollback(owner: &SkillSaveOwner) -> Command {
    match owner {
        SkillSaveOwner::Plain(ticket) => Command::RollbackSkill { ticket: *ticket },
        SkillSaveOwner::Device(ticket) => Command::SkillDevice(SkillDeviceCommand::Rollback {
            operation: ticket.inventory.operation,
        }),
        SkillSaveOwner::AttributeTransfer(ticket) => {
            Command::AttributeTransfer(AttributeTransferCommand::Rollback {
                operation: ticket.inventory.operation,
            })
        }
    }
}
fn commit(owner: &SkillSaveOwner) -> Command {
    match owner {
        SkillSaveOwner::Plain(ticket) => Command::CommitSkill { ticket: *ticket },
        SkillSaveOwner::Device(ticket) => Command::SkillDevice(SkillDeviceCommand::Commit {
            receipt: bace_simulation::InventoryReceipt {
                operation: ticket.inventory.operation,
                revisions: ticket
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .map(|c| (c.after.id, c.after.revision))
                    .collect(),
            },
        }),
        SkillSaveOwner::AttributeTransfer(ticket) => {
            Command::AttributeTransfer(AttributeTransferCommand::Commit {
                receipt: bace_simulation::InventoryReceipt {
                    operation: ticket.inventory.operation,
                    revisions: ticket
                        .inventory
                        .proposal
                        .changes
                        .iter()
                        .map(|change| (change.after.id, change.after.revision))
                        .collect(),
                },
            })
        }
    }
}
#[cfg(test)]
mod tests;
