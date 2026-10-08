//! Bounded same-owner handoff between physical contacts and Magic proc chains.
use super::*;
impl Kernel {
    pub(super) fn service_physical_procs(&mut self) -> Result<(), SimulationError> {
        self.combat.enable_physical_procs();
        for _ in 0..32 {
            let Some(request) = self.combat.pending_physical_proc() else {
                break;
            };
            match self.magic.advance_physical_proc(&request, &self.world) {
                Ok(Some(receipt)) => {
                    self.combat
                        .accept_physical_proc(receipt)
                        .map_err(|_| SimulationError::InvalidCommand)?;
                    self.magic
                        .acknowledge_physical_proc(&receipt)
                        .map_err(|_| SimulationError::InvalidCommand)?;
                    self.combat
                        .resume_physical_procs(&mut self.world, &self.inventory);
                }
                Ok(None)
                | Err(
                    bace_gameplay_api::CastRejection::Capacity
                    | bace_gameplay_api::CastRejection::Busy,
                ) => break,
                Err(_) => return Err(SimulationError::InvalidCommand),
            }
        }
        Ok(())
    }
}
