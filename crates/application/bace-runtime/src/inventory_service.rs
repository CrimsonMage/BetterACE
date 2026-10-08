//! Durable inventory lifecycle: exact claimed proposal, captured baseline,
//! immutable placement, durable resolution, owner adoption, then publication.
mod corpse;
mod equipment;
mod freezing;
mod output;
mod preparation;
use crate::{
    online_player_saves::OnlinePlayerSaveService,
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::{SaveHandle, SaveSubmitError},
    simulation::SimulationInput,
};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::{SaveSnapshot, StorageViewFence};
use bace_simulation::{
    Command, InventoryCommand, InventoryCommandKind, InventoryDecision, InventoryOperation,
    InventoryOutcome, InventoryReceipt, InventoryTicket, PlayerReadSnapshot,
    PlayerSnapshotOperation, PlayerSnapshotOutcome, PlayerSnapshotRequest,
};
pub use output::project_inventory_completion;
pub use preparation::{SplitPreparationInput, prepare_split_request};
use std::{
    collections::BTreeSet,
    sync::{Arc, mpsc::TrySendError},
};
pub struct InventoryWork {
    pub binding: CharacterBinding,
    pub operation: InventoryOperation,
    /// Only a fresh accepted template instance may fill a before=None row.
    pub fresh: Option<crate::stack_factory::PreparedStack>,
    /// Exact world/container metadata, including unchanged destination ancestors.
    pub external: Vec<crate::game_inventory::FrozenInventoryItem>,
    pub storage_views: Vec<StorageViewFence>,
    pub world_epoch: u64,
}
pub struct InventoryCompletion {
    pub work: InventoryWork,
    pub committed: bool,
    pub snapshots: Vec<SaveSnapshot>,
}
enum Phase {
    Barrier,
    CaptureReady,
    Capturing(u64),
    Captured(Arc<PlayerReadSnapshot>, u64),
    PreparingEquipment {
        snapshot: Arc<PlayerReadSnapshot>,
        unix: u64,
        correlation: u64,
        command: Option<Box<Command>>,
    },
    Saving {
        save: Box<PendingPlacementSave>,
        submitted: bool,
    },
    Delivering {
        correlation: u64,
        command: Option<Box<Command>>,
        committed: Option<Vec<SaveSnapshot>>,
    },
    Finishing {
        committed: Option<Vec<SaveSnapshot>>,
    },
}
struct Pending {
    work: InventoryWork,
    phase: Phase,
    baseline_reserved: bool,
}
pub struct InventoryService {
    pending: Option<Pending>,
    completion: Option<InventoryCompletion>,
    blocked: Option<String>,
    last_correlation: u64,
}
impl Default for InventoryService {
    fn default() -> Self {
        Self::new()
    }
}
impl InventoryService {
    pub fn new() -> Self {
        Self {
            pending: None,
            completion: None,
            blocked: None,
            last_correlation: 0,
        }
    }
    pub fn requires_drain(&self) -> bool {
        self.pending.is_some() || self.completion.is_some()
    }
    pub fn blocked(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    pub fn stage(&mut self, work: InventoryWork) -> Result<(), Box<InventoryWork>> {
        if self.requires_drain() || !valid(&work) {
            return Err(Box::new(work));
        }
        self.pending = Some(Pending {
            work,
            phase: Phase::Barrier,
            baseline_reserved: false,
        });
        self.blocked = None;
        Ok(())
    }
    pub fn take_completion(&mut self) -> Option<InventoryCompletion> {
        self.completion.take()
    }
    pub fn retry(&mut self) {
        if self.blocked.is_some()
            && let Some(p) = &mut self.pending
            && let Phase::Delivering {
                correlation,
                command,
                committed,
            } = &mut p.phase
            && command.is_none()
        {
            *command = Some(Box::new(resolve(
                &p.work.operation.ticket,
                *correlation,
                committed.is_some(),
            )));
        }
        self.blocked = None;
    }
    /// Cancellation is legal only before any database submission or while a
    /// definite rejection is being delivered. An uncertain commit is never undone.
    pub fn reject_unsubmitted(&mut self, correlation: u64) -> Result<(), String> {
        if correlation <= self.last_correlation {
            return Err("stale inventory resolution correlation".into());
        }
        let p = self.pending.as_mut().ok_or("no inventory proposal")?;
        if !matches!(
            p.phase,
            Phase::Barrier | Phase::CaptureReady | Phase::Captured(..)
        ) {
            return Err("inventory capture/write must resolve before cancellation".into());
        }
        p.phase = Phase::Delivering {
            correlation,
            command: Some(Box::new(resolve(
                &p.work.operation.ticket,
                correlation,
                false,
            ))),
            committed: None,
        };
        self.last_correlation = correlation;
        self.blocked = None;
        Ok(())
    }
    pub fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| matches!(p.phase,Phase::Capturing(c) if c==outcome.correlation))
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
                self.blocked = Some(format!("inventory capture rejected: {error:?}"));
            }
        }
        Ok(())
    }
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
        let Some(p) = &mut self.pending else {
            return Ok(());
        };
        let mut finished = false;
        let result = (|| -> Result<(), String> {
            match &mut p.phase {
                Phase::Barrier => {
                    if online.critical_ready(&[p.work.binding.actor.0])? {
                        online.begin_critical(&[p.work.binding.actor.0])?;
                        p.baseline_reserved = true;
                        p.phase = Phase::CaptureReady;
                    }
                }
                Phase::CaptureReady => {
                    if correlation <= self.last_correlation {
                        return Err("stale inventory capture correlation".into());
                    }
                    let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                        correlation,
                        binding: p.work.binding,
                        operation: Some((
                            PlayerSnapshotOperation::Inventory(p.work.operation.ticket.operation),
                            p.work.operation.actor_revision,
                        )),
                    });
                    match send(command) {
                        Ok(()) => {
                            self.last_correlation = correlation;
                            p.phase = Phase::Capturing(correlation);
                        }
                        Err(error) => {
                            if matches!(*error, TrySendError::Disconnected(_)) {
                                return Err("inventory snapshot ingress closed".into());
                            }
                        }
                    }
                }
                Phase::Capturing(_) => {}
                Phase::PreparingEquipment { command, .. } => {
                    if let Some(value) = command.take()
                        && let Err(error) = send(*value)
                    {
                        match *error {
                            TrySendError::Full(value) => *command = Some(Box::new(value)),
                            TrySendError::Disconnected(value) => {
                                *command = Some(Box::new(value));
                                return Err("equipment preparation ingress closed".into());
                            }
                        }
                    }
                }
                Phase::Captured(snapshot, unix) => {
                    if p.work.operation.equipment.is_some()
                        && p.work.operation.equipment_vitals.is_none()
                    {
                        return Ok(());
                    }
                    let save = freezing::freeze(&p.work, snapshot, *unix, online)?;
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
                    // Do not consume a terminal receipt without a usable owner token.
                    if correlation <= self.last_correlation {
                        return Err("stale inventory resolution correlation".into());
                    }
                    let Some(result) = save.poll() else {
                        return Ok(());
                    };
                    *submitted = false;
                    let committed = match result {
                        PlacementResolution::Uncertain(error) => return Err(error),
                        PlacementResolution::Rejected(_) => None,
                        PlacementResolution::Committed(_) => {
                            let mut rows = save.operation().snapshots.clone();
                            for row in &mut rows {
                                row.expected_version = row
                                    .expected_version
                                    .checked_add(1)
                                    .ok_or("inventory version overflow")?;
                            }
                            Some(rows)
                        }
                    };
                    let command =
                        resolve(&p.work.operation.ticket, correlation, committed.is_some());
                    self.last_correlation = correlation;
                    p.phase = Phase::Delivering {
                        correlation,
                        command: Some(Box::new(command)),
                        committed,
                    };
                }
                Phase::Delivering { command, .. } => {
                    if let Some(value) = command.take()
                        && let Err(error) = send(*value)
                    {
                        match *error {
                            TrySendError::Full(value) => *command = Some(Box::new(value)),
                            TrySendError::Disconnected(value) => {
                                *command = Some(Box::new(value));
                                return Err("inventory owner ingress closed".into());
                            }
                        }
                    }
                }
                Phase::Finishing { committed } => {
                    if let Some(rows) = committed {
                        online.finish_critical_for(&[p.work.binding.actor.0], rows)?;
                    } else if p.baseline_reserved {
                        online.cancel_critical(&[p.work.binding.actor.0])?;
                    }
                    finished = true;
                }
            }
            Ok(())
        })();
        if finished {
            let p = self.pending.take().expect("finished operation");
            let Phase::Finishing { committed } = p.phase else {
                unreachable!()
            };
            self.completion = Some(InventoryCompletion {
                work: p.work,
                committed: committed.is_some(),
                snapshots: committed.unwrap_or_default(),
            });
        }
        if let Err(error) = &result {
            self.blocked = Some(error.clone());
        }
        result
    }
    /// Exact correlation, complete ticket, and expected resolution are mandatory.
    /// Unrelated outcome ownership is returned to its projection/routing owner.
    pub fn accept_outcome(
        &mut self,
        outcome: InventoryOutcome,
    ) -> Result<(), Box<InventoryOutcome>> {
        let Some(p) = &mut self.pending else {
            return Err(Box::new(outcome));
        };
        if let Phase::PreparingEquipment {
            snapshot,
            unix,
            correlation,
            command,
        } = &p.phase
        {
            if *correlation != outcome.correlation || command.is_some() {
                return Err(Box::new(outcome));
            }
            match &outcome.result {
                Ok(InventoryDecision::EquipmentPrepared(operation))
                    if operation.ticket == p.work.operation.ticket
                        && operation.binding == p.work.binding
                        && operation.actor_revision == p.work.operation.actor_revision
                        && operation.equipment == p.work.operation.equipment
                        && operation.equipment_vitals.is_some() =>
                {
                    p.work.operation = (**operation).clone();
                    p.phase = Phase::Captured(snapshot.clone(), *unix);
                }
                Err(error) => {
                    self.blocked = Some(format!(
                        "equipment physical preparation rejected: {error:?}"
                    ));
                    p.phase = Phase::Captured(snapshot.clone(), *unix);
                }
                _ => return Err(Box::new(outcome)),
            }
            return Ok(());
        }
        let Phase::Delivering {
            correlation,
            command,
            committed,
        } = &mut p.phase
        else {
            return Err(Box::new(outcome));
        };
        if *correlation != outcome.correlation || command.is_some() {
            return Err(Box::new(outcome));
        }
        let matched = match &outcome.result {
            Ok(InventoryDecision::Committed(ticket)) => {
                committed.is_some() && *ticket == p.work.operation.ticket
            }
            Ok(InventoryDecision::Rejected { operation: id }) => {
                committed.is_none() && *id == p.work.operation.ticket.operation
            }
            Err(error) => {
                self.blocked = Some(format!("inventory owner rejected resolution: {error:?}"));
                return Ok(());
            }
            _ => false,
        };
        if !matched {
            return Err(Box::new(outcome));
        }
        p.phase = Phase::Finishing {
            committed: committed.take(),
        };
        Ok(())
    }
}
fn resolve(ticket: &InventoryTicket, correlation: u64, committed: bool) -> Command {
    Command::Inventory(Box::new(InventoryCommand {
        correlation,
        kind: if committed {
            InventoryCommandKind::Commit(InventoryReceipt {
                operation: ticket.operation,
                revisions: ticket
                    .proposal
                    .changes
                    .iter()
                    .map(|c| (c.after.id, c.after.revision))
                    .collect(),
            })
        } else {
            InventoryCommandKind::Reject {
                operation: ticket.operation,
            }
        },
    }))
}
fn valid(work: &InventoryWork) -> bool {
    let p = &work.operation;
    let t = &p.ticket;
    if p.binding != work.binding
        || t.actor != work.binding.actor
        || t.operation == 0
        || p.actor_revision == 0
        || work.world_epoch == 0
        || work.world_epoch > i64::MAX as u64
        || t.proposal.changes.is_empty()
        || t.proposal.changes.len() > 1023
        || t.proposal.participants.len() > 1024
        || work.external.len() > 4096
        || work.storage_views.len() > 1024
        || p.enchantments.values().map(Vec::len).sum::<usize>() > 65536
    {
        return false;
    }
    let ids: BTreeSet<_> = work.external.iter().map(|s| s.entity.object_id).collect();
    if ids.len() != work.external.len() || ids.contains(&0) || ids.contains(&t.actor.0) {
        return false;
    }
    let fresh: Vec<_> = t
        .proposal
        .changes
        .iter()
        .filter(|c| c.before.is_none())
        .collect();
    match (&work.fresh, fresh.as_slice()) {
        (None, []) => true,
        (Some(f), [c]) => {
            f.item.id == c.after.id
                && f.item.template == c.after.template
                && f.item.stack == c.after.stack
                && f.frozen.entity.object_id == c.after.id.0
                && f.frozen.entity.state.weenie_id == c.after.template
                && f.frozen.entity.mutation_revision == 0
                && f.frozen.persisted_version == 0
                && f.frozen.placement.is_none()
                && f.frozen.enchantments.is_empty()
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests;
