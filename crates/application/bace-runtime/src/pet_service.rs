//! One retained pet device transaction; world publication follows the exact receipt.
use crate::{
    online_player_saves::OnlinePlayerSaveService,
    pet_saves::{PetOperationId, PetSaveInput, freeze_pet},
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::{SaveHandle, SaveSubmitError},
    simulation::SimulationInput,
};
use bace_gameplay_api::CharacterBinding;
use bace_magic::EnchantmentEntry;
use bace_persistence::SaveSnapshot;
use bace_simulation::{
    Command, InventoryReceipt, InventoryTicket, PetAction, PetCommand, PetDecision, PetOutcome,
    PlayerReadSnapshot, PlayerSnapshotOperation, PlayerSnapshotOutcome, PlayerSnapshotRequest,
};
use std::sync::{Arc, mpsc::TrySendError};

pub struct PetWork {
    pub binding: CharacterBinding,
    pub ticket: InventoryTicket,
    pub actor_revision: u64,
    pub registry_after: Option<(u64, Vec<EnchantmentEntry>)>,
    pub summon: bool,
    pub identity: PetOperationId,
}

pub struct PetCompletion {
    pub ticket: InventoryTicket,
    pub summon: bool,
    pub committed: bool,
    pub snapshots: Vec<SaveSnapshot>,
}

enum Phase {
    Barrier,
    CaptureReady,
    Capturing(u64),
    Captured(Arc<PlayerReadSnapshot>, u64),
    Saving {
        save: Box<PendingPlacementSave>,
        receipt: InventoryReceipt,
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
    work: PetWork,
    phase: Phase,
    baseline_reserved: bool,
}

pub struct PetService {
    pending: Option<Pending>,
    completion: Option<PetCompletion>,
    blocked: Option<String>,
    last_correlation: u64,
}

impl Default for PetService {
    fn default() -> Self {
        Self::new()
    }
}

impl PetService {
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
    pub fn actor(&self) -> Option<bace_types::EntityId> {
        self.pending
            .as_ref()
            .map(|pending| pending.work.binding.actor)
    }
    pub fn blocked(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    pub fn take_completion(&mut self) -> Option<PetCompletion> {
        self.completion.take()
    }

    pub fn stage(&mut self, work: PetWork) -> Result<(), Box<PetWork>> {
        if self.requires_drain()
            || work.ticket.actor != work.binding.actor
            || work.ticket.operation == 0
            || work.actor_revision == 0
            || !work.summon && work.registry_after.is_some()
            || work
                .registry_after
                .as_ref()
                .is_some_and(|(_, entries)| entries.len() > 4096)
        {
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
        let p = self.pending.as_mut().expect("matched pet capture");
        match outcome.result {
            Ok(snapshot) if snapshot.binding() == p.work.binding => {
                p.phase = Phase::Captured(snapshot, unix)
            }
            Err(error) => {
                p.phase = Phase::CaptureReady;
                self.blocked = Some(format!("pet snapshot rejected: {error:?}"));
            }
            _ => return Err(outcome),
        }
        Ok(())
    }

    pub fn owns_outcome(&self, outcome: &PetOutcome) -> bool {
        self.pending.as_ref().is_some_and(|p| matches!(p.phase, Phase::Delivering { correlation, .. } if correlation == outcome.correlation))
    }

    pub fn accept_outcome(&mut self, outcome: PetOutcome) -> Result<(), Box<PetOutcome>> {
        if !self.owns_outcome(&outcome) {
            return Err(Box::new(outcome));
        }
        let p = self.pending.as_mut().expect("matched pet outcome");
        let Phase::Delivering {
            command, committed, ..
        } = &mut p.phase
        else {
            unreachable!()
        };
        if command.is_some() {
            return Err(Box::new(outcome));
        }
        match outcome.result {
            Ok(PetDecision::Resolved {
                operation,
                committed: result,
            }) if operation == p.work.ticket.operation && result == committed.is_some() => {
                p.phase = Phase::Finishing {
                    committed: committed.take(),
                };
            }
            Err(error) => self.blocked = Some(format!("pet receipt adoption rejected: {error:?}")),
            _ => return Err(Box::new(outcome)),
        }
        Ok(())
    }

    pub fn retry(&mut self) {
        self.blocked = None;
        if let Some(p) = &mut self.pending
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
    }

    pub fn poll(
        &mut self,
        input: &SimulationInput,
        online: &mut OnlinePlayerSaveService,
        saves: &SaveHandle,
        correlation: u64,
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
                        return Err("stale pet capture correlation".into());
                    }
                    let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                        correlation,
                        binding: p.work.binding,
                        operation: Some((
                            PlayerSnapshotOperation::Pet(p.work.ticket.operation),
                            p.work.actor_revision,
                        )),
                    });
                    match input.try_submit(command) {
                        Ok(()) => {
                            self.last_correlation = correlation;
                            p.phase = Phase::Capturing(correlation);
                        }
                        Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => {
                            return Err("pet snapshot ingress closed".into());
                        }
                    }
                }
                Phase::Capturing(_) => {}
                Phase::Captured(snapshot, unix) => {
                    let (baseline, version, lease) = online
                        .baseline(p.work.binding.actor.0)
                        .ok_or("pet player baseline missing")?;
                    let items = online.operation_inventory_baselines(snapshot)?;
                    let captured_items = online.operation_inventory_changes(snapshot)?;
                    let frozen = freeze_pet(PetSaveInput {
                        id: p.work.identity,
                        ticket: &p.work.ticket,
                        snapshot,
                        player_baseline: baseline,
                        player_version: version,
                        lease,
                        items: &items,
                        captured_items: &captured_items,
                        captured_unix_millis: *unix,
                        registry_after: p
                            .work
                            .registry_after
                            .as_ref()
                            .map(|(revision, after)| (*revision, after.as_slice())),
                        summon: p.work.summon,
                    })?;
                    p.phase = Phase::Saving {
                        save: Box::new(
                            PendingPlacementSave::new(frozen.operation)
                                .map_err(|e| e.to_string())?,
                        ),
                        receipt: frozen.receipt,
                        submitted: false,
                    };
                }
                Phase::Saving {
                    save,
                    receipt,
                    submitted,
                } => {
                    if !*submitted {
                        match save.submit(saves) {
                            Ok(()) => *submitted = true,
                            Err(SaveSubmitError::Full) => {}
                            Err(error) => return Err(error.to_string()),
                        }
                        return Ok(());
                    }
                    if correlation <= self.last_correlation {
                        return Err("stale pet resolution correlation".into());
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
                                    .ok_or("pet persisted version overflow")?;
                            }
                            Some(rows)
                        }
                    };
                    let command = resolve(&p.work.ticket, correlation, committed.is_some());
                    debug_assert_eq!(receipt.operation, p.work.ticket.operation);
                    self.last_correlation = correlation;
                    p.phase = Phase::Delivering {
                        correlation,
                        command: Some(Box::new(command)),
                        committed,
                    };
                }
                Phase::Delivering { command, .. } => {
                    if let Some(value) = command.take() {
                        match input.try_submit(*value) {
                            Ok(()) => {}
                            Err(TrySendError::Full(value)) => *command = Some(Box::new(value)),
                            Err(TrySendError::Disconnected(value)) => {
                                *command = Some(Box::new(value));
                                return Err("pet receipt ingress closed".into());
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
            let p = self.pending.take().expect("finished pet save");
            let Phase::Finishing { committed } = p.phase else {
                unreachable!()
            };
            self.completion = Some(PetCompletion {
                ticket: p.work.ticket,
                summon: p.work.summon,
                committed: committed.is_some(),
                snapshots: committed.unwrap_or_default(),
            });
        }
        if let Err(error) = &result {
            self.blocked = Some(error.clone());
        }
        result
    }
}

fn resolve(ticket: &InventoryTicket, correlation: u64, committed: bool) -> Command {
    Command::Pet(PetCommand {
        correlation,
        action: PetAction::Resolve {
            receipt: InventoryReceipt {
                operation: ticket.operation,
                revisions: ticket
                    .proposal
                    .changes
                    .iter()
                    .map(|change| (change.after.id, change.after.revision))
                    .collect(),
            },
            committed,
        },
    })
}
