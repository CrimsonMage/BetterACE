//! One retained crafting operation: owner proposal, immutable capture, atomic
//! database receipt, owner adoption, and only then a publishable completion.
mod freezing;
use crate::{
    online_player_saves::OnlinePlayerSaveService,
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::{SaveHandle, SaveSubmitError},
    simulation::SimulationInput,
};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::SaveSnapshot;
use bace_simulation::{
    Command, CraftingCommand, CraftingCommandKind, CraftingDecision, CraftingOutcome,
    CraftingResult, CraftingTicket, InventoryReceipt, PlayerReadSnapshot, PlayerSnapshotOperation,
    PlayerSnapshotOutcome, PlayerSnapshotRequest,
};
use bace_storage_codec::EntitySaveV1;
use std::{
    collections::BTreeSet,
    sync::{Arc, mpsc::TrySendError},
};

pub struct CraftingWork {
    pub binding: CharacterBinding,
    pub ticket: CraftingTicket,
    /// Exact accepted source templates for newly allocated salvage bags only.
    pub generated: Vec<EntitySaveV1>,
}
pub struct CraftingCompletion {
    pub work: CraftingWork,
    pub committed: bool,
}
enum Phase {
    Barrier,
    CaptureReady,
    Capturing(u64),
    Captured(Arc<PlayerReadSnapshot>, u64),
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
    work: CraftingWork,
    phase: Phase,
    baseline_reserved: bool,
}
pub struct CraftingService {
    pending: Option<Pending>,
    completion: Option<CraftingCompletion>,
    blocked: Option<String>,
    last_correlation: u64,
}
impl Default for CraftingService {
    fn default() -> Self {
        Self::new()
    }
}
impl CraftingService {
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
    pub fn stage(&mut self, work: CraftingWork) -> Result<(), Box<CraftingWork>> {
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
    pub fn take_completion(&mut self) -> Option<CraftingCompletion> {
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
                &p.work.ticket,
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
            return Err("stale crafting resolution correlation".into());
        }
        let p = self.pending.as_mut().ok_or("no crafting proposal")?;
        if !matches!(
            p.phase,
            Phase::Barrier | Phase::CaptureReady | Phase::Captured(..)
        ) {
            return Err("crafting capture/write must resolve before cancellation".into());
        }
        p.phase = Phase::Delivering {
            correlation,
            command: Some(Box::new(resolve(&p.work.ticket, correlation, false))),
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
                self.blocked = Some(format!("crafting capture rejected: {error:?}"));
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
                        return Err("stale crafting capture correlation".into());
                    }
                    let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                        correlation,
                        binding: p.work.binding,
                        operation: Some((
                            PlayerSnapshotOperation::Crafting(p.work.ticket.operation),
                            revision(&p.work.ticket),
                        )),
                    });
                    match send(command) {
                        Ok(()) => {
                            self.last_correlation = correlation;
                            p.phase = Phase::Capturing(correlation);
                        }
                        Err(error) => {
                            if matches!(*error, TrySendError::Disconnected(_)) {
                                return Err("crafting snapshot ingress closed".into());
                            }
                        }
                    }
                }
                Phase::Capturing(_) => {}
                Phase::Captured(snapshot, unix) => {
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
                        return Err("stale crafting resolution correlation".into());
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
                                    .ok_or("crafting version overflow")?;
                            }
                            Some(rows)
                        }
                    };
                    let command = resolve(&p.work.ticket, correlation, committed.is_some());
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
                                return Err("crafting owner ingress closed".into());
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
            self.completion = Some(CraftingCompletion {
                work: p.work,
                committed: committed.is_some(),
            });
        }
        if let Err(error) = &result {
            self.blocked = Some(error.clone());
        }
        result
    }
    /// Exact correlation, complete ticket, and expected resolution are mandatory.
    /// Unrelated outcome ownership is returned to its projection/routing owner.
    pub fn accept_outcome(&mut self, outcome: CraftingOutcome) -> Result<(), Box<CraftingOutcome>> {
        let Some(p) = &mut self.pending else {
            return Err(Box::new(outcome));
        };
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
            Ok(CraftingResult::Committed(ticket)) => {
                committed.is_some() && **ticket == p.work.ticket
            }
            Ok(CraftingResult::RolledBack(id)) => {
                committed.is_none() && *id == p.work.ticket.operation
            }
            Err(error) => {
                self.blocked = Some(format!("crafting owner rejected resolution: {error:?}"));
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
fn revision(ticket: &CraftingTicket) -> u64 {
    match &ticket.decision {
        CraftingDecision::Tinker(p) => p.expected_actor_revision,
        CraftingDecision::Salvage(p) => p.expected_actor_revision,
    }
}
fn resolve(ticket: &CraftingTicket, correlation: u64, committed: bool) -> Command {
    Command::Crafting(CraftingCommand {
        correlation,
        action: if committed {
            CraftingCommandKind::Commit {
                receipt: InventoryReceipt {
                    operation: ticket.operation,
                    revisions: ticket
                        .inventory
                        .changes
                        .iter()
                        .map(|c| (c.after.id, c.after.revision))
                        .collect(),
                },
            }
        } else {
            CraftingCommandKind::Rollback {
                operation: ticket.operation,
            }
        },
    })
}
fn valid(work: &CraftingWork) -> bool {
    let ticket = &work.ticket;
    if ticket.operation == 0
        || ticket.actor != work.binding.actor
        || revision(ticket) == 0
        || ticket.inventory.changes.len() > 1023
        || ticket.inventory.participants.len() > 1024
        || ticket.registry_reservations.len() > 1024
        || work.generated.len() > 300
    {
        return false;
    }
    let ids: BTreeSet<_> = work.generated.iter().map(|s| s.object_id).collect();
    if ids.len() != work.generated.len() || ids.contains(&0) || ids.contains(&ticket.actor.0) {
        return false;
    }
    match &ticket.decision {
        CraftingDecision::Tinker(p) => {
            p.actor == ticket.actor.0 && p.operation_id != [0; 16] && work.generated.is_empty()
        }
        CraftingDecision::Salvage(p) => {
            p.actor == ticket.actor.0
                && p.operation_id != [0; 16]
                && p.bags.len() == work.generated.len()
                && p.consumed.len() <= 300
        }
    }
}
#[cfg(test)]
pub(crate) mod tests;
