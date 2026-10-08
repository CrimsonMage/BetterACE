//! One bounded ammunition transaction retains its exact snapshot, database
//! operation and owner decision. Projectile flight begins only after the receipt.
mod freezing;
use crate::{
    online_player_saves::OnlinePlayerSaveService,
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::{SaveHandle, SaveSubmitError},
    simulation::SimulationInput,
};
use bace_gameplay_api::{
    CharacterBinding, InventoryRejection, weapon_combat::PhysicalLaunchProposal,
};
use bace_persistence::SaveSnapshot;
use bace_simulation::{
    Command, PhysicalResourceAction as Action, PhysicalResourceCommand,
    PhysicalResourceDecision as Decision, PhysicalResourceOutcome, PhysicalResourceTicket,
};
use std::sync::Arc;
enum Phase {
    Barrier,
    Prepare,
    WaitingPrepare(u64),
    Saving {
        save: Box<PendingPlacementSave>,
        submitted: bool,
    },
    Resolve,
    WaitingResolve(u64),
    Baseline,
    Acknowledge,
    WaitingAcknowledge(u64),
    Abort,
    WaitingAbort(u64),
    AbortBaseline,
}
struct Pending {
    appearance_prepared: bool,
    binding: CharacterBinding,
    launch: PhysicalLaunchProposal,
    ticket: Option<Arc<PhysicalResourceTicket>>,
    phase: Phase,
    committed: Option<Vec<SaveSnapshot>>,
}
pub struct PhysicalResourceCompletion {
    pub failure: Option<String>,
    pub binding: CharacterBinding,
    pub launch: PhysicalLaunchProposal,
    pub ticket: Option<Arc<PhysicalResourceTicket>>,
    pub committed: bool,
}
pub struct PhysicalResourceService {
    pending: Option<Pending>,
    completion: Option<PhysicalResourceCompletion>,
    next: u64,
    failure: Option<String>,
}
impl Default for PhysicalResourceService {
    fn default() -> Self {
        Self::new()
    }
}
impl PhysicalResourceService {
    pub fn new() -> Self {
        Self {
            pending: None,
            completion: None,
            next: 0x5048_0000_0000_0000,
            failure: None,
        }
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some() || self.completion.is_some()
    }
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    pub fn actor(&self) -> Option<bace_types::EntityId> {
        self.pending.as_ref().map(|p| p.binding.actor)
    }
    pub fn stage(
        &mut self,
        binding: CharacterBinding,
        launch: PhysicalLaunchProposal,
    ) -> Result<(), Box<PhysicalLaunchProposal>> {
        if self.has_pending()
            || binding.actor.0 != launch.actor
            || launch.consumed != 1
            || launch.event_id == [0; 16]
        {
            return Err(Box::new(launch));
        }
        self.pending = Some(Pending {
            appearance_prepared: false,
            binding,
            launch,
            ticket: None,
            phase: Phase::Barrier,
            committed: None,
        });
        self.failure = None;
        Ok(())
    }
    pub fn appearance_ticket(&self) -> Option<&Arc<PhysicalResourceTicket>> {
        self.pending
            .as_ref()
            .filter(|p| !p.appearance_prepared)
            .and_then(|p| p.ticket.as_ref())
    }
    /// Renderer preparation uses this exact ticket's frozen ammunition source.
    /// No valuable write is admitted before its immutable public model exists.
    pub fn acknowledge_appearance(&mut self, operation: u64) -> Result<(), String> {
        let pending = self
            .pending
            .as_mut()
            .filter(|p| p.launch.operation == operation && p.ticket.is_some())
            .ok_or("ammunition appearance operation mismatch")?;
        pending.appearance_prepared = true;
        Ok(())
    }
    pub fn owns(&self, outcome: &PhysicalResourceOutcome) -> bool {
        self.pending.as_ref().is_some_and(|p| match p.phase {
            Phase::WaitingPrepare(id)
            | Phase::WaitingResolve(id)
            | Phase::WaitingAcknowledge(id)
            | Phase::WaitingAbort(id) => id == outcome.correlation,
            _ => false,
        })
    }
    pub fn accept(
        &mut self,
        outcome: Arc<PhysicalResourceOutcome>,
        online: &OnlinePlayerSaveService,
        unix: u64,
    ) -> Result<(), Arc<PhysicalResourceOutcome>> {
        if !self.owns(&outcome) {
            return Err(outcome);
        }
        let p = self.pending.as_mut().expect("matched resource owner");
        match (&p.phase, &outcome.result) {
            (Phase::WaitingPrepare(_), Ok(Decision::Prepared(ticket)))
                if ticket.binding == p.binding && ticket.launch == p.launch =>
            {
                p.ticket = Some(ticket.clone());
                match freezing::freeze(ticket, online, unix) {
                    Ok(save) => {
                        p.phase = Phase::Saving {
                            save: Box::new(save),
                            submitted: false,
                        }
                    }
                    Err(error) => {
                        self.failure = Some(error);
                        p.phase = Phase::Resolve;
                    }
                }
            }
            (Phase::WaitingPrepare(_), Err(error)) => {
                // No durable proposal has been accepted. Preserve the launch and
                // reserved baseline; source revision errors require explicit review.
                self.failure = Some(format!("ammunition reservation: {error:?}"));
                p.phase = if matches!(
                    error,
                    InventoryRejection::Busy
                        | InventoryRejection::Capacity
                        | InventoryRejection::DurabilityPending
                ) {
                    Phase::Prepare
                } else {
                    Phase::Abort
                };
            }
            (
                Phase::WaitingResolve(_),
                Ok(Decision::Resolved {
                    operation,
                    committed,
                }),
            ) if *operation == p.launch.operation && *committed == p.committed.is_some() => {
                p.phase = Phase::Baseline
            }
            (Phase::WaitingResolve(_), Err(error)) => {
                self.failure = Some(format!("ammunition owner adoption: {error:?}"));
                p.phase = Phase::Resolve;
            }
            (Phase::WaitingAcknowledge(_), Ok(Decision::Acknowledged { operation }))
                if *operation == p.launch.operation =>
            {
                let p = self.pending.take().expect("matched terminal owner");
                self.completion = Some(PhysicalResourceCompletion {
                    failure: if p.committed.is_some() {
                        None
                    } else {
                        self.failure.take()
                    },
                    binding: p.binding,
                    launch: p.launch,
                    ticket: p.ticket,
                    committed: p.committed.is_some(),
                });
                self.failure = None;
            }
            (Phase::WaitingAcknowledge(_), Err(error)) => {
                self.failure = Some(format!("ammunition acknowledgment: {error:?}"));
                p.phase = Phase::Acknowledge;
            }
            (Phase::WaitingAbort(_), Ok(Decision::Aborted { operation }))
                if *operation == p.launch.operation =>
            {
                p.phase = Phase::AbortBaseline
            }
            (Phase::WaitingAbort(_), Err(error)) => {
                self.failure = Some(format!("ammunition abort: {error:?}"));
                p.phase = Phase::Abort;
            }
            _ => return Err(outcome),
        }
        Ok(())
    }
    pub fn poll(
        &mut self,
        input: &SimulationInput,
        online: &mut OnlinePlayerSaveService,
        saves: &SaveHandle,
    ) -> Result<(), String> {
        let Some(p) = &mut self.pending else {
            return Ok(());
        };
        let action = match &mut p.phase {
            Phase::Barrier => {
                if online.critical_ready(&[p.binding.actor.0])? {
                    online.begin_critical(&[p.binding.actor.0])?;
                    p.phase = Phase::Prepare;
                }
                None
            }
            Phase::Prepare => Some(Action::Prepare {
                binding: p.binding,
                launch: Box::new(p.launch.clone()),
            }),
            Phase::Resolve => Some(Action::Resolve {
                operation: p.launch.operation,
                committed: p.committed.is_some(),
            }),
            Phase::Acknowledge => Some(Action::Acknowledge {
                operation: p.launch.operation,
            }),
            Phase::Abort => Some(Action::Abort {
                binding: p.binding,
                operation: p.launch.operation,
            }),
            Phase::AbortBaseline => {
                online.cancel_critical(&[p.binding.actor.0])?;
                let p = self.pending.take().expect("aborted operation owner");
                self.completion = Some(PhysicalResourceCompletion {
                    failure: self.failure.take(),
                    binding: p.binding,
                    launch: p.launch,
                    ticket: None,
                    committed: false,
                });
                return Ok(());
            }
            Phase::Saving { save, submitted } => {
                if !p.appearance_prepared {
                    return Ok(());
                }
                if !*submitted {
                    match save.submit(saves) {
                        Ok(()) => *submitted = true,
                        Err(SaveSubmitError::Full) => return Ok(()),
                        Err(error) => {
                            self.failure = Some(format!("ammunition save admission: {error:?}"));
                            return Err(self.failure.clone().expect("set"));
                        }
                    }
                }
                if let Some(resolution) = save.poll() {
                    *submitted = false;
                    match resolution {
                        PlacementResolution::Uncertain(error) => {
                            self.failure = Some(error.clone());
                            return Err(error);
                        }
                        PlacementResolution::Rejected(error) => {
                            self.failure =
                                Some(format!("ammunition transaction rejected: {error:?}"));
                            p.phase = Phase::Resolve;
                        }
                        PlacementResolution::Committed(_) => {
                            let mut rows = save.operation().snapshots.clone();
                            for row in &mut rows {
                                row.expected_version = row
                                    .expected_version
                                    .checked_add(1)
                                    .ok_or("ammunition durable version overflow")?;
                            }
                            p.committed = Some(rows);
                            p.phase = Phase::Resolve;
                        }
                    }
                }
                None
            }
            Phase::Baseline => {
                if let Some(rows) = &p.committed {
                    online.finish_critical_for(&[p.binding.actor.0], rows)?;
                } else {
                    online.cancel_critical(&[p.binding.actor.0])?;
                }
                p.phase = Phase::Acknowledge;
                None
            }
            Phase::WaitingPrepare(_)
            | Phase::WaitingResolve(_)
            | Phase::WaitingAcknowledge(_)
            | Phase::WaitingAbort(_) => None,
        };
        if let Some(action) = action {
            let correlation = self
                .next
                .checked_add(1)
                .ok_or("ammunition correlation overflow")?;
            match input.try_submit(Command::PhysicalResource(Box::new(
                PhysicalResourceCommand {
                    correlation,
                    action,
                },
            ))) {
                Ok(()) => {
                    self.next = correlation;
                    p.phase = match p.phase {
                        Phase::Prepare => Phase::WaitingPrepare(correlation),
                        Phase::Resolve => Phase::WaitingResolve(correlation),
                        Phase::Acknowledge => Phase::WaitingAcknowledge(correlation),
                        Phase::Abort => Phase::WaitingAbort(correlation),
                        _ => unreachable!("action only from ready phase"),
                    };
                }
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    return Err("ammunition simulation owner closed; receipt retained".into());
                }
            }
        }
        Ok(())
    }
    pub fn take_completion(&mut self) -> Option<PhysicalResourceCompletion> {
        self.completion.take()
    }
}

#[cfg(test)]
mod tests;
