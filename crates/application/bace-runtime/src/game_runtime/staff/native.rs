//! Native actor program preparation precedes the actual authorized cast action.
use super::*;
use crate::staff_native_magic::{StaffNativeMagicWork, StaffNativeMagicWorker};
pub(in crate::game_runtime) struct NativePending {
    request: StaffRequest,
    command: Box<StaffCommand>,
    phase: NativePhase,
}
enum NativePhase {
    Prepare(Arc<StaffNativeMagicWork>),
    Preparing(Arc<StaffNativeMagicWork>),
    Program(Option<Box<bace_simulation::PreparedActorMagicProgram>>),
    Awaiting,
}
impl GameRuntime {
    pub(super) fn prepare_staff_native(
        &self,
        pending: &Pending,
        request: &StaffRequest,
        command: Box<StaffCommand>,
        snapshot: &bace_simulation::PlayerReadSnapshot,
        unix: u64,
    ) -> Result<Phase, String> {
        let bace_gameplay_api::staff::StaffAction::CastSpell { spell, .. } = command.action else {
            return Ok(Phase::Command(command));
        };
        let session = self
            .sessions
            .get(&pending.key)
            .ok_or("native cast session missing")?;
        let (baseline, _, _) = self
            .online_saves
            .baseline(pending.context.actor.0)
            .ok_or("native cast player source missing")?;
        let saved = crate::player_saves::freeze_player_snapshot(baseline, snapshot, unix)
            .map_err(|e| e.to_string())?;
        let mut writer = bace_wire::Writer::new();
        writer
            .string16(session.account.name.as_str())
            .map_err(|_| "native account is not representable as CP1252")?;
        let encoded = writer.into_bytes();
        let length = usize::from(u16::from_le_bytes([encoded[0], encoded[1]]));
        let account_cp1252 = encoded[2..2 + length].to_vec();
        let top_level_templates=snapshot.items().iter().filter(|item|matches!(item.place,bace_inventory::ItemPlace::Contained{container,..} if container==pending.context.actor)).map(|item|item.template).collect();
        let row = Arc::new(
            self.assets
                .spell_rows
                .get(&spell)
                .ok_or("native spell server row missing")?
                .clone(),
        );
        let work = Arc::new(StaffNativeMagicWork {
            token: pending.token,
            binding: snapshot.binding(),
            expected_character_revision: snapshot.character().progression().revision(),
            source: Arc::new(saved.player.entity.state.clone()),
            account_cp1252,
            top_level_templates,
            row,
        });
        Ok(Phase::Native(Box::new(NativePending {
            request: request.clone(),
            command,
            phase: NativePhase::Prepare(work),
        })))
    }
    pub(super) fn poll_staff_native(&mut self) -> Result<(), String> {
        if let Some(worker) = &mut self.staff.native
            && let Some(result) = worker.poll()?
        {
            let pending = self
                .staff
                .pending
                .as_mut()
                .ok_or("native program has no staff owner")?;
            let Phase::Native(native) = &mut pending.phase else {
                return Err("native program phase mismatch".into());
            };
            if !matches!(&native.phase,NativePhase::Preparing(work) if Arc::ptr_eq(work,&result.work))
            {
                return Err("native program correlation".into());
            }
            match result.result {
                Ok(program) => native.phase = NativePhase::Program(Some(Box::new(program))),
                Err(error) => {
                    native.phase = NativePhase::Prepare(result.work);
                    self.staff.failure = Some(error.clone());
                    return Err(error);
                }
            }
        }
        let Some(pending) = &mut self.staff.pending else {
            return Ok(());
        };
        let Phase::Native(native) = &mut pending.phase else {
            return Ok(());
        };
        match &mut native.phase {
            NativePhase::Prepare(work) => {
                if self.staff.native.is_none() {
                    self.staff.native = Some(StaffNativeMagicWorker::start(
                        self.bootstrap.assets.clone(),
                        self.bootstrap.pack.generation.clone(),
                    )?);
                }
                if self
                    .staff
                    .native
                    .as_mut()
                    .expect("native worker")
                    .submit(work.clone())
                    .is_ok()
                {
                    native.phase = NativePhase::Preparing(work.clone());
                }
            }
            NativePhase::Program(program) => {
                let command = bace_simulation::Command::ActorMagicProgram {
                    correlation: pending.token,
                    program: program.take().ok_or("native program missing")?,
                };
                match self.simulation.input().try_submit(command) {
                    Ok(()) => native.phase = NativePhase::Awaiting,
                    Err(error) => {
                        let (closed, command) = match error {
                            std::sync::mpsc::TrySendError::Full(command) => (false, command),
                            std::sync::mpsc::TrySendError::Disconnected(command) => (true, command),
                        };
                        let bace_simulation::Command::ActorMagicProgram {
                            program: retained, ..
                        } = command
                        else {
                            unreachable!()
                        };
                        *program = Some(retained);
                        if closed {
                            return Err("native program owner ingress closed".into());
                        }
                    }
                }
            }
            NativePhase::Preparing(_) | NativePhase::Awaiting => {}
        }
        Ok(())
    }
    pub(super) fn accept_staff_native_outcome(
        &mut self,
        event: &StaffEvent,
    ) -> Result<bool, String> {
        let Some(pending) = &mut self.staff.pending else {
            return Ok(false);
        };
        let Phase::Native(native) = &pending.phase else {
            return Ok(false);
        };
        let StaffEvent::Outcome {
            token,
            actor,
            result,
        } = event
        else {
            return Ok(false);
        };
        if !matches!(native.phase, NativePhase::Awaiting)
            || *token != pending.token
            || *actor != Some(pending.context.actor)
        {
            return Err("native program receipt correlation".into());
        }
        pending.phase = match result {
            Ok(()) => Phase::Command(native.command.clone()),
            Err(error) => {
                // Cold fingerprint/motion work can outlive an ordinary age tick.
                // Recapture and re-prepare without consuming the action sequence.
                if *error != bace_gameplay_api::staff::StaffError::Stale {
                    self.staff.failure = Some(format!("native program rejected: {error:?}"));
                }
                Phase::Capture(native.request.clone())
            }
        };
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
