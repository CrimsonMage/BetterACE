//! Exact corpse/reward completions are acknowledged without losing output on pressure.
use super::*;
use bace_simulation::PveServiceOutcome;
#[derive(Default)]
pub struct RecoveredPveServices {
    pub outcomes: Vec<PveServiceOutcome>,
}
impl RecoveredPveServices {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct PveServiceReceivers {
    outcomes: mpsc::Receiver<PveServiceOutcome>,
}
pub(super) struct PveServiceSenders {
    outcomes: mpsc::SyncSender<PveServiceOutcome>,
}
pub(super) fn channels(capacity: usize) -> (PveServiceSenders, PveServiceReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(capacity);
    (
        PveServiceSenders { outcomes },
        PveServiceReceivers { outcomes: receiver },
    )
}
impl PveServiceSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_pve_service_outcome() else {
                break;
            };
            if self.outcomes.try_send(*outcome).is_err() {
                break;
            }
            kernel.take_pve_service_outcome();
        }
    }
}
impl PveServiceReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredPveServices) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn pve_service_outcomes(&self) -> &mpsc::Receiver<PveServiceOutcome> {
        &self.pve_services.outcomes
    }
}
