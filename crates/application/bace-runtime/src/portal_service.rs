//! Retained portal/lifestone transaction, with separate receipt and effect adoption.
mod freezing;
use crate::{
    online_player_saves::OnlinePlayerSaveService,
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::{SaveHandle, SaveSubmitError},
};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::SaveSnapshot;
use bace_simulation::{
    Command, PlayerReadSnapshot, PlayerSnapshotOperation, PlayerSnapshotOutcome,
    PlayerSnapshotRequest, PortalResolution, PortalResolutionCommand, PortalResolutionOutcome,
    PortalServiceEvent, PortalServiceOrigin, PortalServiceTicket,
};
use std::{
    collections::BTreeSet,
    sync::{Arc, mpsc::TrySendError},
};

pub struct PortalWork {
    pub epoch: u64,
    pub bindings: Vec<CharacterBinding>,
    pub ticket: PortalServiceTicket,
}
pub struct PortalCompletion {
    pub work: PortalWork,
    pub committed: bool,
    pub aborted: bool,
}
enum Phase {
    Barrier,
    CaptureReady,
    Capturing(u64),
    Freeze,
    Saving {
        save: Box<PendingPlacementSave>,
        submitted: bool,
    },
    Delivering {
        correlation: u64,
        resolution: PortalResolution,
        submitted: bool,
        committed: Option<Vec<SaveSnapshot>>,
    },
    Effect {
        committed: Vec<SaveSnapshot>,
    },
    Finishing {
        committed: Option<Vec<SaveSnapshot>>,
        aborted: bool,
    },
}
struct Pending {
    work: PortalWork,
    actors: Vec<u32>,
    captures: Vec<(Arc<PlayerReadSnapshot>, u64)>,
    reserved: bool,
    early_effect: Option<bool>,
    phase: Phase,
}
pub struct PortalService {
    pending: Option<Pending>,
    completion: Option<PortalCompletion>,
    blocked: Option<String>,
    last_correlation: u64,
}
impl Default for PortalService {
    fn default() -> Self {
        Self::new()
    }
}
impl PortalService {
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
    pub fn contains_actor(&self, actor: bace_types::EntityId) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.actors.contains(&actor.0))
            || self
                .completion
                .as_ref()
                .is_some_and(|c| c.work.bindings.iter().any(|b| b.actor == actor))
    }
    pub fn blocked(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    pub fn take_completion(&mut self) -> Option<PortalCompletion> {
        self.completion.take()
    }
    pub fn stage(&mut self, work: PortalWork) -> Result<(), Box<PortalWork>> {
        let actors: BTreeSet<_> = work.bindings.iter().map(|b| b.actor.0).collect();
        if self.requires_drain()
            || work.epoch == 0
            || work.ticket.operation == 0
            || !(1..=9).contains(&work.bindings.len())
            || actors.len() != work.bindings.len()
            || work.bindings.len() != work.ticket.participants.len()
            || matches!(
                work.ticket.origin,
                PortalServiceOrigin::Emote { .. } | PortalServiceOrigin::Death
            )
            || work
                .ticket
                .participants
                .iter()
                .map(|(id, _, _)| id.0)
                .collect::<BTreeSet<_>>()
                != actors
            || work.ticket.participants.iter().any(|(id, before, after)| {
                !actors.contains(&id.0) || before.checked_add(1) != Some(*after)
            })
        {
            return Err(Box::new(work));
        }
        self.pending = Some(Pending {
            actors: actors.into_iter().collect(),
            work,
            captures: Vec::new(),
            reserved: false,
            early_effect: None,
            phase: Phase::Barrier,
        });
        self.blocked = None;
        Ok(())
    }
    pub fn retry(&mut self) {
        if self.blocked.is_some()
            && let Some(p) = &mut self.pending
            && let Phase::Delivering { submitted, .. } = &mut p.phase
        {
            *submitted = false;
        }
        self.blocked = None;
    }
    pub fn reject_unsubmitted(&mut self, correlation: u64) -> Result<(), String> {
        if correlation <= self.last_correlation {
            return Err("stale portal cancellation correlation".into());
        }
        let p = self.pending.as_mut().ok_or("no portal operation")?;
        if !matches!(
            p.phase,
            Phase::Barrier | Phase::CaptureReady | Phase::Freeze
        ) {
            return Err("portal capture/write must resolve before cancellation".into());
        }
        p.phase = Phase::Delivering {
            correlation,
            resolution: resolution(&p.work.ticket, false),
            submitted: false,
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
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if !self.owns_capture(&outcome) {
            return Err(outcome);
        }
        let p = self.pending.as_mut().expect("matched portal capture");
        match &outcome.result {
            Ok(snapshot) => {
                let binding = p.work.bindings[p.captures.len()];
                let before = p
                    .work
                    .ticket
                    .participants
                    .iter()
                    .find(|(id, _, _)| *id == binding.actor)
                    .expect("validated portal participant")
                    .1;
                if snapshot.binding() != binding
                    || snapshot.operation()
                        != Some((
                            PlayerSnapshotOperation::Portal(p.work.ticket.operation),
                            before,
                        ))
                {
                    return Err(outcome);
                }
                p.captures.push((snapshot.clone(), unix));
                p.phase = if p.captures.len() == p.work.bindings.len() {
                    Phase::Freeze
                } else {
                    Phase::CaptureReady
                };
            }
            Err(error) => {
                self.blocked = Some(format!("portal capture rejected: {error:?}"));
                p.phase = Phase::CaptureReady;
            }
        }
        Ok(())
    }
    /// One bounded step. Caller supplies a monotonically allocated correlation;
    /// no timer, database wait or success projection happens on this adapter path.
    pub fn poll_with<F>(
        &mut self,
        correlation: u64,
        online: &mut OnlinePlayerSaveService,
        saves: &SaveHandle,
        mut send: F,
    ) -> Result<(), String>
    where
        F: FnMut(Command) -> Result<(), Box<TrySendError<Command>>>,
    {
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
                    if online.critical_ready(&p.actors)? {
                        online.begin_critical(&p.actors)?;
                        p.reserved = true;
                        p.phase = Phase::CaptureReady;
                    }
                }
                Phase::CaptureReady => {
                    if correlation <= self.last_correlation {
                        return Err("stale portal capture correlation".into());
                    }
                    let binding = p.work.bindings[p.captures.len()];
                    let before = p
                        .work
                        .ticket
                        .participants
                        .iter()
                        .find(|(id, _, _)| *id == binding.actor)
                        .expect("validated participant")
                        .1;
                    match send(Command::PlayerSnapshot(PlayerSnapshotRequest {
                        correlation,
                        binding,
                        operation: Some((
                            PlayerSnapshotOperation::Portal(p.work.ticket.operation),
                            before,
                        )),
                    })) {
                        Ok(()) => {
                            self.last_correlation = correlation;
                            p.phase = Phase::Capturing(correlation);
                        }
                        Err(error) => {
                            if matches!(*error, TrySendError::Disconnected(_)) {
                                return Err("portal capture owner closed".into());
                            }
                        }
                    }
                }
                Phase::Capturing(_) => {}
                Phase::Freeze => {
                    p.phase = Phase::Saving {
                        save: Box::new(freezing::freeze(&p.work, &p.captures, online)?),
                        submitted: false,
                    };
                }
                Phase::Saving { save, submitted } => {
                    if !*submitted {
                        match save.submit(saves) {
                            Ok(()) => *submitted = true,
                            Err(SaveSubmitError::Full) => {}
                            Err(error) => return Err(error.to_string()),
                        };
                        return Ok(());
                    }
                    if correlation <= self.last_correlation {
                        return Err("stale portal receipt correlation".into());
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
                                    .ok_or("portal version overflow")?;
                            }
                            Some(rows)
                        }
                    };
                    self.last_correlation = correlation;
                    p.phase = Phase::Delivering {
                        correlation,
                        resolution: resolution(&p.work.ticket, committed.is_some()),
                        submitted: false,
                        committed,
                    };
                }
                Phase::Delivering {
                    correlation,
                    resolution,
                    submitted,
                    ..
                } => {
                    if !*submitted {
                        match send(Command::PortalResolution(PortalResolutionCommand {
                            correlation: *correlation,
                            resolution: resolution.clone(),
                        })) {
                            Ok(()) => *submitted = true,
                            Err(error) => {
                                if matches!(*error, TrySendError::Disconnected(_)) {
                                    return Err("portal resolution owner closed".into());
                                }
                            }
                        }
                    }
                }
                Phase::Effect { .. } => {}
                Phase::Finishing { committed, .. } => {
                    if let Some(rows) = committed {
                        online.finish_critical_for(&p.actors, rows)?;
                    } else if p.reserved {
                        online.cancel_critical(&p.actors)?;
                    }
                    finished = true;
                }
            }
            Ok(())
        })();
        if finished {
            let p = self.pending.take().expect("finished portal");
            let Phase::Finishing { committed, aborted } = p.phase else {
                unreachable!()
            };
            self.completion = Some(PortalCompletion {
                work: p.work,
                committed: committed.is_some(),
                aborted,
            });
        }
        if let Err(error) = &result {
            self.blocked = Some(error.clone());
        }
        result
    }
    pub fn accept_resolution(
        &mut self,
        outcome: PortalResolutionOutcome,
    ) -> Result<(), PortalResolutionOutcome> {
        let Some(p) = &mut self.pending else {
            return Err(outcome);
        };
        let Phase::Delivering {
            correlation,
            resolution,
            submitted,
            committed,
        } = &mut p.phase
        else {
            return Err(outcome);
        };
        if *correlation != outcome.correlation || *resolution != outcome.resolution || !*submitted {
            return Err(outcome);
        }
        if let Err(error) = outcome.result {
            self.blocked = Some(format!("portal resolution rejected: {error:?}"));
            return Ok(());
        }
        p.phase = match committed.take() {
            Some(committed) => match p.early_effect.take() {
                Some(aborted) => Phase::Finishing {
                    committed: Some(committed),
                    aborted,
                },
                None => Phase::Effect { committed },
            },
            None => Phase::Finishing {
                committed: None,
                aborted: false,
            },
        };
        Ok(())
    }
    /// Borrowed event remains the output owner's responsibility. A commit marker
    /// alone cannot release baselines: physical/character adoption must follow.
    pub fn accept_effect(&mut self, event: &PortalServiceEvent) -> bool {
        let Some(p) = &mut self.pending else {
            return false;
        };
        if !matches!(
            &p.phase,
            Phase::Effect { .. }
                | Phase::Delivering {
                    committed: Some(_),
                    submitted: true,
                    ..
                }
        ) {
            return false;
        }
        let operation = p.work.ticket.operation;
        let aborted = match (&p.work.ticket.effect, event) {
            (
                bace_simulation::PortalServiceEffect::Link(_)
                | bace_simulation::PortalServiceEffect::Sanctuary { .. },
                PortalServiceEvent::Linked {
                    operation: actual,
                    actor,
                },
            ) if *actual == operation && *actor == p.work.ticket.actor => false,
            (
                bace_simulation::PortalServiceEffect::Summon { entity, .. },
                PortalServiceEvent::Summoned {
                    operation: actual,
                    entity: actual_entity,
                    template,
                },
            ) if *actual == operation
                && entity == actual_entity
                && matches!(p.work.ticket.effect,bace_simulation::PortalServiceEffect::Summon{template:expected,..} if expected==*template) =>
            {
                false
            }
            (
                bace_simulation::PortalServiceEffect::Teleport(moves),
                PortalServiceEvent::Teleported {
                    operation: actual,
                    actors,
                    ..
                },
            ) if *actual == operation
                && moves.len() == actors.len()
                && moves.iter().zip(actors).all(|(m, a)| m.actor == *a) =>
            {
                false
            }
            (
                bace_simulation::PortalServiceEffect::Teleport(_),
                PortalServiceEvent::AbortedAfterCommit {
                    operation: actual,
                    actor,
                },
            ) if *actual == operation && *actor == p.work.ticket.actor => true,
            _ => return false,
        };
        if let Phase::Effect { committed } = &mut p.phase {
            p.phase = Phase::Finishing {
                committed: Some(std::mem::take(committed)),
                aborted,
            };
        } else {
            // Independent bounded output channels may deliver the effect before
            // the receipt acknowledgment. Retain evidence until both arrive.
            p.early_effect = Some(aborted);
        }
        true
    }
}
fn resolution(ticket: &PortalServiceTicket, committed: bool) -> PortalResolution {
    if committed {
        PortalResolution::Commit(bace_simulation::PortalServiceReceipt {
            operation: ticket.operation,
            actor: ticket.actor,
            after_revision: ticket.after_revision,
            revisions: ticket
                .participants
                .iter()
                .map(|(id, _, after)| (*id, *after))
                .collect(),
        })
    } else {
        PortalResolution::Reject {
            operation: ticket.operation,
            actor: ticket.actor,
        }
    }
}

#[cfg(test)]
mod tests;
