//! Exact item-owner casts mediate proc continuations; queue admission is not a
//! completed cloak effect and cannot release damage that is waiting for it.
use super::*;
use bace_gameplay_api::{CastChange, CastRejection, CastRequest};
impl Kernel {
    pub fn register_magic_damage_spell_flags(
        &mut self,
        spell: u32,
        flags: u32,
    ) -> Result<(), CastRejection> {
        self.magic.register_damage_spell_flags(spell, flags)
    }
    pub(super) fn service_item_magic_procs(&mut self) -> Result<(), SimulationError> {
        self.item_proc_blocked.clear();
        for _ in 0..32 {
            let mut completed = false;
            for index in 0..self.item_proc_inflight.len() {
                let inflight = self.item_proc_inflight[index];
                let Some(outcome) = self.magic.take_item_proc_outcome(inflight) else {
                    continue;
                };
                completed = true;
                if matches!(outcome.result, Ok(CastChange::Started { .. })) {
                    break;
                }
                let request = self
                    .magic
                    .item_proc_request(inflight)
                    .ok_or(SimulationError::InvalidCommand)?;
                self.magic
                    .confirm_item_proc(request)
                    .map_err(|_| SimulationError::InvalidCommand)?;
                self.item_proc_inflight.remove(index);
                break;
            }
            if completed {
                continue;
            }
            if self.item_proc_inflight.len() == 32 {
                break;
            }
            self.item_proc_exclusions.clear();
            self.item_proc_exclusions
                .extend(self.item_proc_inflight.iter().copied());
            self.item_proc_exclusions
                .extend(self.item_proc_blocked.iter().copied());
            let Some(request) = self
                .magic
                .pending_item_proc_excluding(&self.item_proc_exclusions)
            else {
                break;
            };
            match self.cast_from_server(
                request.origin,
                CastRequest::Targeted {
                    target: request.target,
                    spell: request.spell,
                },
            ) {
                Ok(_) => self.item_proc_inflight.push(request.origin),
                Err(CastRejection::Busy | CastRejection::Capacity) => {
                    self.item_proc_blocked.push(request.origin)
                }
                Err(_) => {
                    // A terminal failed instant cast still releases the source
                    // continuation; it never fabricates the missing spell effect.
                    self.magic
                        .confirm_item_proc(request)
                        .map_err(|_| SimulationError::InvalidCommand)?;
                }
            }
        }
        Ok(())
    }
}
