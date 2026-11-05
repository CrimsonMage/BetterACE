//! One retained healing-kit operation. The kit, player vital and stamina are
//! frozen in one revision-fenced durable placement before owner adoption.
use crate::{
    game_inventory::{InventoryFreezeInput, freeze_inventory},
    online_player_saves::OnlinePlayerSaveService,
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::{SaveHandle, SaveSubmitError},
    simulation::SimulationInput,
};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::SaveSnapshot;
use bace_simulation::{
    Command, HealingKitCommand, HealingKitDecision, HealingKitOutcome, HealingKitTicket,
    InventoryReceipt, PlayerReadSnapshot, PlayerSnapshotOperation, PlayerSnapshotOutcome,
    PlayerSnapshotRequest,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc::TrySendError},
};

pub struct HealingKitCompletion {
    pub ticket: HealingKitTicket,
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
    ticket: HealingKitTicket,
    phase: Phase,
    reserved: bool,
}
pub struct HealingKitService {
    pending: Option<Pending>,
    completion: Option<HealingKitCompletion>,
    blocked: Option<String>,
    last_capture: u64,
}
impl HealingKitService {
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
    pub fn take_completion(&mut self) -> Option<HealingKitCompletion> {
        self.completion.take()
    }
    pub fn stage(
        &mut self,
        identity: [u8; 16],
        binding: CharacterBinding,
        ticket: HealingKitTicket,
    ) -> Result<(), Box<HealingKitTicket>> {
        if self.has_pending()
            || identity == [0; 16]
            || binding.actor != ticket.context.actor
            || binding.actor != ticket.vitals.first().map_or(binding.actor, |v| v.actor)
        {
            return Err(Box::new(ticket));
        }
        let mut operation = String::from("healing-kit:");
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
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if !self.owns_capture(&outcome) {
            return Err(outcome);
        }
        let p = self.pending.as_mut().expect("matched capture");
        match outcome.result {
            Ok(snapshot) if snapshot.binding() == p.binding => {
                p.phase = Phase::Captured(snapshot, unix)
            }
            Err(error) => {
                p.phase = Phase::CaptureReady;
                self.blocked = Some(format!("healing capture: {error:?}"));
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
    pub fn accept_owner(
        &mut self,
        outcome: HealingKitOutcome,
    ) -> Result<(), Box<HealingKitOutcome>> {
        let Some(p) = self.pending.as_mut() else {
            return Err(Box::new(outcome));
        };
        let waiting = matches!(p.phase, Phase::Delivering { command: None, .. });
        if !waiting {
            return Err(Box::new(outcome));
        }
        let committed = matches!(p.phase, Phase::Delivering { rows: Some(_), .. });
        match &outcome.result {
            Ok(HealingKitDecision::Committed(ticket))
                if committed
                    && ticket.operation == p.ticket.operation
                    && ticket.context == p.ticket.context => {}
            Ok(HealingKitDecision::Rejected(operation))
                if !committed && *operation == p.ticket.operation => {}
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
                        return Err("healing stale capture correlation".into());
                    }
                    let request = PlayerSnapshotRequest {
                        correlation,
                        binding: p.binding,
                        operation: Some((
                            PlayerSnapshotOperation::HealingKit(p.ticket.operation),
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
                            return Err("healing snapshot input closed".into());
                        }
                    }
                }
                Phase::Capturing(_) => {}
                Phase::Captured(snapshot, unix) => {
                    let save = freeze(&p.identity, p.binding, &p.ticket, snapshot, *unix, online)?;
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
                                    command: Some(Box::new(Command::HealingKit(
                                        HealingKitCommand::Reject {
                                            correlation: p.ticket.operation,
                                            operation: p.ticket.operation,
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
                                        .ok_or("healing version overflow")?;
                                }
                                let receipt =
                                    p.ticket
                                        .inventory
                                        .as_ref()
                                        .map(|inventory| InventoryReceipt {
                                            operation: inventory.operation,
                                            revisions: inventory
                                                .proposal
                                                .changes
                                                .iter()
                                                .map(|c| (c.after.id, c.after.revision))
                                                .collect(),
                                        });
                                p.phase = Phase::Delivering {
                                    command: Some(Box::new(Command::HealingKit(
                                        HealingKitCommand::Commit {
                                            correlation: p.ticket.operation,
                                            operation: p.ticket.operation,
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
                                return Err("healing owner input closed".into());
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
                    self.completion = Some(HealingKitCompletion {
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
    ticket: &HealingKitTicket,
    snapshot: &PlayerReadSnapshot,
    unix: u64,
    online: &OnlinePlayerSaveService,
) -> Result<PendingPlacementSave, String> {
    let (baseline, version, lease) = online
        .baseline(binding.actor.0)
        .ok_or("healing baseline missing")?;
    let mut player = crate::player_saves::freeze_player_operation_baseline(
        baseline,
        snapshot,
        PlayerSnapshotOperation::HealingKit(ticket.operation),
        ticket.actor_revision,
        unix,
    )
    .map_err(|e| e.to_string())?;
    if player.player.entity.mutation_revision != ticket.actor_revision || version <= 0 {
        return Err("healing player revision mismatch".into());
    }
    for change in &ticket.vitals {
        if change.actor != binding.actor {
            return Err("healing cross-player vital unsupported".into());
        }
        let id = match change.vital {
            bace_entity::EntityVital::Health => 1,
            bace_entity::EntityVital::Stamina => 3,
            bace_entity::EntityVital::Mana => 5,
        };
        let current = player
            .player
            .entity
            .state
            .properties
            .secondary_attributes
            .iter_mut()
            .find(|v| v.id == id)
            .ok_or("healing saved vital missing")?;
        if current.value.current_level != change.before {
            return Err("healing saved vital baseline mismatch".into());
        }
        current.value.current_level = change.after;
    }
    player.player.entity.mutation_revision = ticket
        .actor_revision
        .checked_add(1)
        .ok_or("healing player revision overflow")?;
    player.validate().map_err(|e| e.to_string())?;
    let player_row = SaveSnapshot {
        object_id: binding.actor.0,
        mutation_revision: player.player.entity.mutation_revision,
        expected_version: version,
        bytes: player.encode().map_err(|e| e.to_string())?,
    };
    let mut operation = if let Some(inventory) = &ticket.inventory {
        let items = online.operation_inventory_baselines(snapshot)?;
        freeze_inventory(InventoryFreezeInput {
            operation_id: identity,
            proposal: &inventory.proposal,
            items: &items,
            other_snapshots: &[player_row],
            leases: &[lease],
            storage_views: &[],
            admitted_positions: &BTreeMap::new(),
        })
        .map_err(|e| e.to_string())?
    } else {
        bace_persistence::PlacementOperation {
            operation_id: identity.to_owned(),
            snapshots: vec![player_row],
            participants: vec![binding.actor.0],
            leases: vec![lease],
            changes: Vec::new(),
            storage_views: Vec::new(),
        }
    };
    for row in online.operation_inventory_changes(snapshot)? {
        if operation
            .snapshots
            .iter()
            .any(|r| r.object_id == row.object_id)
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
