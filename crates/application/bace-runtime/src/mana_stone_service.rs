//! Durable item-mana placement and exact simulation receipt.
use crate::{
    game_inventory::{InventoryFreezeInput, freeze_inventory},
    online_player_saves::OnlinePlayerSaveService,
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::{SaveHandle, SaveSubmitError},
    simulation::SimulationInput,
};
use bace_gameplay_api::CharacterBinding;
use bace_inventory::{ManaChange, ManaStoneDecision};
use bace_persistence::SaveSnapshot;
use bace_simulation::{
    Command, InventoryReceipt, ManaStoneCommand, ManaStoneOutcome, ManaStoneResult,
    ManaStoneTicket, PlayerReadSnapshot, PlayerSnapshotOperation, PlayerSnapshotOutcome,
    PlayerSnapshotRequest,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc::TrySendError},
};

pub struct ManaStoneCompletion {
    pub ticket: ManaStoneTicket,
    pub committed: bool,
}
enum Phase {
    Barrier,
    CaptureReady,
    Capturing(u64),
    Captured(Arc<PlayerReadSnapshot>),
    Saving {
        save: Box<PendingPlacementSave>,
        submitted: bool,
    },
    Delivering {
        command: Option<Box<Command>>,
        rows: Option<Vec<SaveSnapshot>>,
    },
    Finishing {
        rows: Option<Vec<SaveSnapshot>>,
    },
}
struct Pending {
    identity: String,
    binding: CharacterBinding,
    ticket: ManaStoneTicket,
    phase: Phase,
    reserved: bool,
}
pub struct ManaStoneService {
    pending: Option<Pending>,
    completion: Option<ManaStoneCompletion>,
    blocked: Option<String>,
    last_capture: u64,
}
impl ManaStoneService {
    pub fn new() -> Self {
        Self {
            pending: None,
            completion: None,
            blocked: None,
            last_capture: 0,
        }
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some() || self.completion.is_some()
    }
    pub fn blocked(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    pub fn retry(&mut self) {
        self.blocked = None;
    }
    pub fn take_completion(&mut self) -> Option<ManaStoneCompletion> {
        self.completion.take()
    }
    pub fn stage(
        &mut self,
        identity: [u8; 16],
        binding: CharacterBinding,
        ticket: ManaStoneTicket,
    ) -> Result<(), Box<ManaStoneTicket>> {
        if self.has_pending() || identity == [0; 16] || ticket.context.actor != binding.actor {
            return Err(Box::new(ticket));
        }
        let mut operation = String::from("mana-stone:");
        use std::fmt::Write;
        for byte in identity {
            write!(operation, "{byte:02x}").expect("String write");
        }
        self.pending = Some(Pending {
            identity: operation,
            binding,
            ticket,
            phase: Phase::Barrier,
            reserved: false,
        });
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
    ) -> Result<(), PlayerSnapshotOutcome> {
        if !self.owns_capture(&outcome) {
            return Err(outcome);
        }
        let p = self.pending.as_mut().expect("matching capture");
        match outcome.result {
            Ok(snapshot) if snapshot.binding() == p.binding => p.phase = Phase::Captured(snapshot),
            Err(error) => {
                p.phase = Phase::CaptureReady;
                self.blocked = Some(format!("mana-stone capture: {error:?}"));
            }
            other => {
                return Err(PlayerSnapshotOutcome {
                    correlation: outcome.correlation,
                    result: other,
                });
            }
        }
        Ok(())
    }
    pub fn accept_owner(&mut self, outcome: ManaStoneOutcome) -> Result<(), Box<ManaStoneOutcome>> {
        let Some(p) = self.pending.as_mut() else {
            return Err(Box::new(outcome));
        };
        let Phase::Delivering {
            command: None,
            rows,
        } = &p.phase
        else {
            return Err(Box::new(outcome));
        };
        let committed = rows.is_some();
        match &outcome.result {
            Ok(ManaStoneResult::Committed(ticket))
                if committed && ticket.inventory.operation == p.ticket.inventory.operation => {}
            Ok(ManaStoneResult::Rejected(operation))
                if !committed && *operation == p.ticket.inventory.operation => {}
            _ => return Err(Box::new(outcome)),
        }
        let old = std::mem::replace(&mut p.phase, Phase::Barrier);
        let Phase::Delivering { rows, .. } = old else {
            unreachable!()
        };
        p.phase = Phase::Finishing { rows };
        Ok(())
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
        let Some(p) = self.pending.as_mut() else {
            return Ok(());
        };
        let mut finished = false;
        let result = (|| -> Result<(), String> {
            match &mut p.phase {
                Phase::Barrier => {
                    if online.critical_ready(&[p.binding.actor.0])? {
                        online.begin_critical(&[p.binding.actor.0])?;
                        p.reserved = true;
                        p.phase = Phase::CaptureReady;
                    }
                }
                Phase::CaptureReady => {
                    if correlation == 0 || correlation <= self.last_capture {
                        return Err("stale mana-stone capture".into());
                    }
                    let request = PlayerSnapshotRequest {
                        correlation,
                        binding: p.binding,
                        operation: Some((
                            PlayerSnapshotOperation::Inventory(p.ticket.inventory.operation),
                            p.ticket.actor_revision,
                        )),
                    };
                    match input.try_submit(Command::PlayerSnapshot(request)) {
                        Ok(()) => {
                            self.last_capture = correlation;
                            p.phase = Phase::Capturing(correlation);
                        }
                        Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => {
                            return Err("mana-stone snapshot input closed".into());
                        }
                    }
                }
                Phase::Capturing(_) => {}
                Phase::Captured(snapshot) => {
                    let save = freeze(&p.identity, p.binding, &p.ticket, snapshot, online)?;
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
                    } else if let Some(result) = save.poll() {
                        *submitted = false;
                        match result {
                            PlacementResolution::Uncertain(error) => return Err(error),
                            PlacementResolution::Rejected(_) => {
                                p.phase = Phase::Delivering {
                                    command: Some(Box::new(Command::ManaStone(
                                        ManaStoneCommand::Reject {
                                            correlation: p.ticket.inventory.operation,
                                            operation: p.ticket.inventory.operation,
                                        },
                                    ))),
                                    rows: None,
                                }
                            }
                            PlacementResolution::Committed(_) => {
                                let mut rows = save.operation().snapshots.clone();
                                for row in &mut rows {
                                    row.expected_version = row
                                        .expected_version
                                        .checked_add(1)
                                        .ok_or("mana-stone version overflow")?;
                                }
                                let receipt = InventoryReceipt {
                                    operation: p.ticket.inventory.operation,
                                    revisions: p
                                        .ticket
                                        .inventory
                                        .proposal
                                        .changes
                                        .iter()
                                        .map(|change| (change.after.id, change.after.revision))
                                        .collect(),
                                };
                                p.phase = Phase::Delivering {
                                    command: Some(Box::new(Command::ManaStone(
                                        ManaStoneCommand::Commit {
                                            correlation: p.ticket.inventory.operation,
                                            receipt,
                                        },
                                    ))),
                                    rows: Some(rows),
                                };
                            }
                        }
                    }
                }
                Phase::Delivering { command, .. } => {
                    if let Some(value) = command.take() {
                        match input.try_submit(*value) {
                            Ok(()) => {}
                            Err(TrySendError::Full(value)) => *command = Some(Box::new(value)),
                            Err(TrySendError::Disconnected(value)) => {
                                *command = Some(Box::new(value));
                                return Err("mana-stone owner input closed".into());
                            }
                        }
                    }
                }
                Phase::Finishing { rows } => {
                    if let Some(rows) = rows {
                        online.finish_critical(rows)?;
                    } else if p.reserved {
                        online.cancel_critical(&[p.binding.actor.0])?;
                    }
                    self.completion = Some(ManaStoneCompletion {
                        ticket: p.ticket.clone(),
                        committed: rows.is_some(),
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
}

fn freeze(
    identity: &str,
    binding: CharacterBinding,
    ticket: &ManaStoneTicket,
    snapshot: &PlayerReadSnapshot,
    online: &OnlinePlayerSaveService,
) -> Result<PendingPlacementSave, String> {
    let (_, version, lease) = online
        .baseline(binding.actor.0)
        .ok_or("mana-stone player baseline missing")?;
    if version <= 0
        || snapshot.operation()
            != Some((
                PlayerSnapshotOperation::Inventory(ticket.inventory.operation),
                ticket.actor_revision,
            ))
    {
        return Err("mana-stone snapshot operation mismatch".into());
    }
    let mut items = online.operation_inventory_baselines(snapshot)?;
    for change in mana_changes(&ticket.decision) {
        let row = items
            .iter_mut()
            .find(|row| row.entity.object_id == change.item.0)
            .ok_or("mana-stone changed item absent")?;
        let before = row
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 107)
            .map(|p| p.value);
        if before != change.before.current {
            return Err("mana-stone current mana baseline mismatch".into());
        }
        match change.after.current {
            Some(value) => {
                if let Some(p) = row
                    .entity
                    .state
                    .properties
                    .ints
                    .iter_mut()
                    .find(|p| p.id == 107)
                {
                    p.value = value;
                } else {
                    row.entity
                        .state
                        .properties
                        .ints
                        .push(bace_content::Property { id: 107, value });
                }
            }
            None => row.entity.state.properties.ints.retain(|p| p.id != 107),
        }
    }
    let mut operation = freeze_inventory(InventoryFreezeInput {
        operation_id: identity,
        proposal: &ticket.inventory.proposal,
        items: &items,
        other_snapshots: &[],
        leases: &[lease],
        storage_views: &[],
        admitted_positions: &BTreeMap::new(),
    })
    .map_err(|e| e.to_string())?;
    for row in online.operation_inventory_changes(snapshot)? {
        if operation
            .snapshots
            .iter()
            .any(|existing| existing.object_id == row.object_id)
        {
            continue;
        }
        operation.participants.push(row.object_id);
        operation.snapshots.push(row);
    }
    operation.snapshots.sort_by_key(|row| row.object_id);
    operation.participants.sort_unstable();
    operation.participants.dedup();
    PendingPlacementSave::new(operation).map_err(|e| e.to_string())
}
fn mana_changes(decision: &ManaStoneDecision) -> Vec<ManaChange> {
    match decision {
        ManaStoneDecision::Extract { stone, .. } => vec![*stone],
        ManaStoneDecision::Pour { stone, targets, .. } => {
            let mut rows = Vec::with_capacity(targets.len() + 1);
            rows.push(*stone);
            rows.extend(targets);
            rows
        }
        _ => Vec::new(),
    }
}
