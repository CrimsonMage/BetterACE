//! Exact player-death durability, followed by delayed authoritative respawn.
//! A storage failure keeps the dead owner and its frozen operation; it cannot
//! manufacture a resurrection or acknowledge an inventory loss independently.
mod freezing;
use crate::{
    game_inventory::FrozenInventoryItem,
    online_player_saves::OnlinePlayerSaveService,
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::{SaveHandle, SaveSubmitError},
};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::SaveSnapshot;
use bace_simulation::{
    Command, PlayerDeathCommand, PlayerDeathEvent, PlayerDeathServiceCommand,
    PlayerDeathServiceOutcome, PlayerDeathTicket, PlayerReadSnapshot, PlayerSnapshotOperation,
    PlayerSnapshotOutcome, PlayerSnapshotRequest,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc::TrySendError},
};

pub struct PlayerDeathWork {
    pub epoch: u64,
    pub binding: CharacterBinding,
    pub ticket: PlayerDeathTicket,
    /// Only never-persisted corpse/split/coin template instances. Existing item
    /// state always comes from the operation-scoped accepted capture.
    pub fresh_items: Vec<FrozenInventoryItem>,
    pub positions: BTreeMap<u32, bace_content::Position>,
}
pub struct PlayerDeathCompletion {
    pub work: PlayerDeathWork,
    /// Full durable rows, including the corpse metadata needed by expiry.
    pub committed: Vec<SaveSnapshot>,
}
enum Phase {
    Barrier,
    CaptureReady,
    Capturing(u64),
    Freeze,
    Saving {
        frozen: Box<freezing::Frozen>,
        submitted: bool,
        rejected: bool,
    },
    Delivering {
        correlation: u64,
        command: Option<PlayerDeathCommand>,
        committed: Vec<SaveSnapshot>,
        submitted: bool,
    },
    Respawning {
        committed: Vec<SaveSnapshot>,
    },
    Finishing {
        committed: Vec<SaveSnapshot>,
    },
}
struct Pending {
    work: PlayerDeathWork,
    capture: Option<(Arc<PlayerReadSnapshot>, u64)>,
    phase: Phase,
    early_respawn: bool,
}
pub struct PlayerDeathService {
    pending: Option<Pending>,
    completion: Option<PlayerDeathCompletion>,
    blocked: Option<String>,
    last_correlation: u64,
}
impl Default for PlayerDeathService {
    fn default() -> Self {
        Self::new()
    }
}
impl PlayerDeathService {
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
    pub fn contains_actor(&self, actor: bace_types::EntityId) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.work.binding.actor == actor)
            || self
                .completion
                .as_ref()
                .is_some_and(|p| p.work.binding.actor == actor)
    }
    pub fn take_completion(&mut self) -> Option<PlayerDeathCompletion> {
        self.completion.take()
    }
    pub fn stage(&mut self, work: PlayerDeathWork) -> Result<(), Box<PlayerDeathWork>> {
        if self.requires_drain()
            || work.epoch == 0
            || work.ticket.operation == 0
            || work.binding.actor != work.ticket.actor
            || work.fresh_items.len() > 1024
            || work.positions.len() > 1024
            || work.ticket.before_revision.checked_add(1) != Some(work.ticket.after_revision)
        {
            return Err(Box::new(work));
        }
        self.pending = Some(Pending {
            work,
            capture: None,
            phase: Phase::Barrier,
            early_respawn: false,
        });
        self.blocked = None;
        Ok(())
    }
    /// Transfers the routine barrier already held for the cold preparation.
    /// The initial capture proves that exact reserved actor/operation; a fresh
    /// post-proposal capture still supplies the authoritative registry baseline.
    pub fn stage_reserved(
        &mut self,
        work: PlayerDeathWork,
        proof: &PlayerReadSnapshot,
        online: &OnlinePlayerSaveService,
    ) -> Result<(), Box<PlayerDeathWork>> {
        if proof.binding() != work.binding
            || proof.operation()
                != Some((
                    PlayerSnapshotOperation::PlayerDeath(work.ticket.operation),
                    work.ticket.before_revision,
                ))
            || online.operation_inventory_baselines(proof).is_err()
        {
            return Err(Box::new(work));
        }
        self.stage(work)?;
        self.pending.as_mut().expect("staged death").phase = Phase::CaptureReady;
        Ok(())
    }
    pub fn retry(&mut self) -> Result<bool, String> {
        if self.blocked.is_none() {
            return Ok(false);
        }
        if let Some(p) = &mut self.pending
            && let Phase::Saving {
                frozen,
                rejected: true,
                ..
            } = &mut p.phase
        {
            frozen.save =
                PendingPlacementSave::new_world(bace_persistence::WorldPlacementOperation {
                    world_epoch: p.work.epoch,
                    inventory: frozen.save.operation().clone(),
                })
                .map_err(|e| e.to_string())?;
            if let Phase::Saving { rejected, .. } = &mut p.phase {
                *rejected = false;
            }
        }
        self.blocked = None;
        Ok(true)
    }
    pub fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| matches!(p.phase, Phase::Capturing(id) if id == outcome.correlation))
    }
    pub fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if !self.owns_capture(&outcome) {
            return Err(outcome);
        }
        let p = self.pending.as_mut().expect("matched death capture");
        match &outcome.result {
            Ok(snapshot) => {
                if snapshot.binding() != p.work.binding
                    || snapshot.operation()
                        != Some((
                            PlayerSnapshotOperation::PlayerDeath(p.work.ticket.operation),
                            p.work.ticket.before_revision,
                        ))
                {
                    return Err(outcome);
                }
                p.capture = Some((snapshot.clone(), unix));
                p.phase = Phase::Freeze;
            }
            Err(error) => {
                self.blocked = Some(format!("death capture rejected: {error:?}"));
                p.phase = Phase::CaptureReady;
            }
        }
        Ok(())
    }
    pub fn owns_outcome(&self, outcome: &PlayerDeathServiceOutcome) -> bool {
        self.pending.as_ref().is_some_and(|p| matches!(p.phase, Phase::Delivering { correlation, submitted: true, .. } if correlation == outcome.correlation))
    }
    pub fn accept_outcome(
        &mut self,
        outcome: PlayerDeathServiceOutcome,
    ) -> Result<(), Box<PlayerDeathServiceOutcome>> {
        if !self.owns_outcome(&outcome) {
            return Err(Box::new(outcome));
        }
        let p = self.pending.as_mut().expect("matched death outcome");
        let Phase::Delivering {
            command,
            committed,
            submitted,
            ..
        } = &mut p.phase
        else {
            unreachable!()
        };
        match outcome.result {
            Err((error, original)) => {
                *command = Some(*original);
                *submitted = false;
                self.blocked = Some(format!("durable death adoption rejected: {error:?}"));
            }
            Ok(()) => {
                let rows = std::mem::take(committed);
                p.phase = if p.early_respawn {
                    Phase::Finishing { committed: rows }
                } else {
                    Phase::Respawning { committed: rows }
                };
            }
        }
        Ok(())
    }
    /// Events stay with the presentation owner. A receipt alone is insufficient
    /// to release the player: its animation and physical respawn must finish.
    pub fn observe_event(&mut self, event: &PlayerDeathEvent) -> bool {
        let Some(p) = &mut self.pending else {
            return false;
        };
        if !matches!(event, PlayerDeathEvent::Respawned { operation, actor, .. } if *operation == p.work.ticket.operation && *actor == p.work.binding.actor)
        {
            return false;
        }
        match &mut p.phase {
            Phase::Delivering {
                submitted: true, ..
            } => {
                p.early_respawn = true;
                true
            }
            Phase::Respawning { committed } => {
                p.phase = Phase::Finishing {
                    committed: std::mem::take(committed),
                };
                true
            }
            Phase::Finishing { .. } => true,
            _ => false,
        }
    }
    /// One bounded nonblocking transition; the outer event pump supplies time.
    pub fn poll_with<F>(
        &mut self,
        correlation: u64,
        unix: u64,
        tick: u64,
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
                    if online.critical_ready(&[p.work.binding.actor.0])? {
                        online.begin_critical(&[p.work.binding.actor.0])?;
                        p.phase = Phase::CaptureReady;
                    }
                }
                Phase::CaptureReady => {
                    if correlation <= self.last_correlation {
                        return Err("stale death capture correlation".into());
                    }
                    let request = PlayerSnapshotRequest {
                        correlation,
                        binding: p.work.binding,
                        operation: Some((
                            PlayerSnapshotOperation::PlayerDeath(p.work.ticket.operation),
                            p.work.ticket.before_revision,
                        )),
                    };
                    match send(Command::PlayerSnapshot(request)) {
                        Ok(()) => {
                            self.last_correlation = correlation;
                            p.phase = Phase::Capturing(correlation);
                        }
                        Err(error) => {
                            if matches!(*error, TrySendError::Disconnected(_)) {
                                return Err("death capture owner closed".into());
                            }
                        }
                    }
                }
                Phase::Capturing(_) | Phase::Respawning { .. } => {}
                Phase::Freeze => {
                    let (snapshot, unix) = p.capture.as_ref().ok_or("missing death capture")?;
                    p.phase = Phase::Saving {
                        frozen: Box::new(freezing::freeze(&p.work, snapshot, *unix, online)?),
                        submitted: false,
                        rejected: false,
                    };
                }
                Phase::Saving {
                    frozen,
                    submitted,
                    rejected,
                } => {
                    if !*submitted {
                        match frozen.save.submit(saves) {
                            Ok(()) => *submitted = true,
                            Err(SaveSubmitError::Full) => {}
                            Err(error) => return Err(error.to_string()),
                        }
                        return Ok(());
                    }
                    if correlation <= self.last_correlation {
                        return Err("stale death receipt correlation".into());
                    }
                    let expiry = if p.work.ticket.no_corpse.is_some() {
                        if frozen.expires_at != 0 {
                            return Err("NoCorpse expiry snapshot invalid".into());
                        }
                        0
                    } else {
                        crate::player_death_saves::corpse_expiry_tick(
                            frozen.expires_at,
                            i64::try_from(unix / 1000).map_err(|_| "death clock overflow")?,
                            tick,
                        )?
                    };
                    let Some(result) = frozen.save.poll() else {
                        return Ok(());
                    };
                    *submitted = false;
                    match result {
                        PlacementResolution::Uncertain(error) => return Err(error),
                        PlacementResolution::Rejected(error) => {
                            *rejected = true;
                            return Err(format!("death checkpoint rejected: {error}"));
                        }
                        PlacementResolution::Committed(_) => {
                            let mut committed = frozen.save.operation().snapshots.clone();
                            for row in &mut committed {
                                row.expected_version = row
                                    .expected_version
                                    .checked_add(1)
                                    .ok_or("death version overflow")?;
                            }
                            let command = PlayerDeathCommand::Committed {
                                receipt: frozen.receipt.clone(),
                                corpse_expiry_tick: expiry,
                            };
                            p.phase = Phase::Delivering {
                                correlation,
                                command: Some(command),
                                committed,
                                submitted: false,
                            };
                            self.last_correlation = correlation;
                        }
                    }
                }
                Phase::Delivering {
                    correlation,
                    command,
                    submitted,
                    ..
                } => {
                    if !*submitted {
                        let request = PlayerDeathServiceCommand {
                            correlation: *correlation,
                            command: command.take().ok_or("death command owner missing")?,
                        };
                        match send(Command::PlayerDeathService(request)) {
                            Ok(()) => *submitted = true,
                            Err(error) => {
                                let (closed, rejected) = match *error {
                                    TrySendError::Full(c) => (false, c),
                                    TrySendError::Disconnected(c) => (true, c),
                                };
                                let Command::PlayerDeathService(rejected) = rejected else {
                                    unreachable!("exact rejected command")
                                };
                                *command = Some(rejected.command);
                                if closed {
                                    return Err("death adoption owner closed".into());
                                }
                            }
                        }
                    }
                }
                Phase::Finishing { committed } => {
                    let rows = freezing::online_rows(committed, p.work.ticket.corpse.0)?;
                    online.finish_critical_for(&[p.work.binding.actor.0], &rows)?;
                    finished = true;
                }
            }
            Ok(())
        })();
        if finished {
            let p = self.pending.take().expect("finished death");
            let Phase::Finishing { committed } = p.phase else {
                unreachable!()
            };
            self.completion = Some(PlayerDeathCompletion {
                work: p.work,
                committed,
            });
        }
        if let Err(error) = &result {
            self.blocked = Some(error.clone());
        }
        result
    }
}

#[cfg(test)]
mod tests;
