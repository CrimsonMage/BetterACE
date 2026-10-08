//! Valuable staff spellbook changes share routine-save barriers and snapshots.
use super::*;
use crate::staff_spell_saves::{PendingStaffSpellSave, StaffSpellResolution};
use bace_gameplay_api::staff::{StaffAction, StaffSpellTicket};
use bace_simulation::{
    Command, PlayerSnapshotOperation, PlayerSnapshotOutcome, PlayerSnapshotRequest,
};
pub(super) struct SpellPending {
    pub ticket: StaffSpellTicket,
    phase: SpellPhase,
}
enum SpellPhase {
    Barrier,
    CaptureReady,
    Capturing(u64),
    Captured(Arc<bace_simulation::PlayerReadSnapshot>, u64),
    Saving {
        save: Box<PendingStaffSpellSave>,
        submitted: bool,
    },
    Owner {
        rows: Option<Vec<bace_persistence::SaveSnapshot>>,
        queued: bool,
    },
}
impl SpellPending {
    pub(super) fn new(ticket: StaffSpellTicket) -> Self {
        Self {
            ticket,
            phase: SpellPhase::Barrier,
        }
    }
    pub(super) fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        matches!(self.phase,SpellPhase::Capturing(c) if c==outcome.correlation)
    }
}
impl GameRuntime {
    pub(in crate::game_runtime::staff) fn accept_staff_spell_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), String> {
        let p = self
            .staff
            .spell
            .as_mut()
            .ok_or("missing staff spell capture")?;
        if !p.owns_capture(&outcome) {
            return Err("staff spell capture correlation".into());
        }
        match outcome.result {
            Ok(snapshot) => p.phase = SpellPhase::Captured(snapshot, unix),
            Err(error) => {
                p.phase = SpellPhase::CaptureReady;
                return Err(format!("staff spell capture: {error:?}"));
            }
        }
        Ok(())
    }
    pub(in crate::game_runtime::staff) fn poll_staff_spell(&mut self) -> Result<(), String> {
        let correlation = self.token()?;
        let Some(p) = &mut self.staff.spell else {
            return Ok(());
        };
        let actor = p.ticket.context.actor.0;
        match &mut p.phase {
            SpellPhase::Barrier => {
                if self.online_saves.critical_ready(&[actor])? {
                    self.online_saves.begin_critical(&[actor])?;
                    p.phase = SpellPhase::CaptureReady;
                }
            }
            SpellPhase::CaptureReady => {
                let c = p.ticket.context;
                if self
                    .simulation
                    .input()
                    .try_submit(Command::PlayerSnapshot(PlayerSnapshotRequest {
                        correlation,
                        binding: bace_gameplay_api::CharacterBinding {
                            actor: c.actor,
                            account: c.account,
                            session: c.session,
                        },
                        operation: Some((
                            PlayerSnapshotOperation::StaffSpell(p.ticket.operation),
                            p.ticket.before_revision,
                        )),
                    }))
                    .is_ok()
                {
                    p.phase = SpellPhase::Capturing(correlation);
                }
            }
            SpellPhase::Capturing(_) => {}
            SpellPhase::Captured(snapshot, unix) => {
                let (baseline, version, lease) = self
                    .online_saves
                    .baseline(actor)
                    .ok_or("staff spell baseline missing")?;
                let saved = crate::player_saves::freeze_player_operation_baseline(
                    baseline,
                    snapshot,
                    PlayerSnapshotOperation::StaffSpell(p.ticket.operation),
                    p.ticket.before_revision,
                    *unix,
                )
                .map_err(|e| e.to_string())?;
                let mut id = [0; 16];
                id[..8].copy_from_slice(&self.bootstrap.world_owner.epoch().to_le_bytes());
                id[8..].copy_from_slice(&p.ticket.operation.to_le_bytes());
                let mut save = crate::staff_spell_saves::freeze_staff_spell(
                    id, &p.ticket, &saved, version, lease,
                )
                .map_err(|e| e.to_string())?;
                save.join_captured_inventory(
                    self.online_saves.operation_inventory_changes(snapshot)?,
                )
                .map_err(|e| e.to_string())?;
                p.phase = SpellPhase::Saving {
                    save: Box::new(save),
                    submitted: false,
                };
            }
            SpellPhase::Saving { save, submitted } => {
                if !*submitted {
                    match save.submit(&self.saves.handle) {
                        Ok(()) => *submitted = true,
                        Err(crate::saves::SaveSubmitError::Full) => {}
                        Err(error) => return Err(error.to_string()),
                    }
                    return Ok(());
                }
                if let Some(result) = save.poll() {
                    *submitted = false;
                    match result {
                        StaffSpellResolution::Uncertain(error) => return Err(error),
                        StaffSpellResolution::Rejected { .. } => {
                            p.phase = SpellPhase::Owner {
                                rows: None,
                                queued: false,
                            }
                        }
                        StaffSpellResolution::Committed { .. } => {
                            let mut rows = save.operation().snapshots.clone();
                            for row in &mut rows {
                                row.expected_version += 1;
                            }
                            p.phase = SpellPhase::Owner {
                                rows: Some(rows),
                                queued: false,
                            };
                        }
                    }
                }
            }
            SpellPhase::Owner { rows, queued } => {
                if !*queued {
                    let action = if rows.is_some() {
                        StaffAction::SpellCommitted(p.ticket.clone())
                    } else {
                        StaffAction::SpellRejected(p.ticket.clone())
                    };
                    if self
                        .simulation
                        .input()
                        .try_submit(Command::Staff(StaffCommand {
                            token: p.ticket.operation,
                            action,
                        }))
                        .is_ok()
                    {
                        *queued = true;
                    }
                }
            }
        }
        Ok(())
    }
    pub(in crate::game_runtime::staff) fn finish_staff_spell(
        &mut self,
        token: u64,
        result: Result<(), bace_gameplay_api::staff::StaffError>,
    ) -> Result<bool, String> {
        let Some(p) = &mut self.staff.spell else {
            return Ok(true);
        };
        let SpellPhase::Owner { rows, queued } = &mut p.phase else {
            return Err("staff spell outcome before durable result".into());
        };
        if token != p.ticket.operation || !*queued {
            return Err("staff spell receipt correlation".into());
        }
        if rows.is_some() && result.is_err()
            || rows.is_none() && result != Err(bace_gameplay_api::staff::StaffError::Stale)
        {
            *queued = false;
            self.staff.failure = Some(format!(
                "staff spell owner rejected durable commit: {result:?}"
            ));
            return Ok(false);
        }
        if let Some(rows) = rows {
            self.online_saves.finish_critical(rows)?;
        } else {
            self.online_saves
                .cancel_critical(&[p.ticket.context.actor.0])?;
        }
        self.staff.spell = None;
        Ok(true)
    }
}
