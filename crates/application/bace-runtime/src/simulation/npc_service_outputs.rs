//! Exact corpse/reward completions are acknowledged without losing output on pressure.
use super::*;
use bace_simulation::NpcServiceOutcome;
#[derive(Default)]
pub struct RecoveredNpcServices {
    pub outcomes: Vec<Arc<NpcServiceOutcome>>,
}
impl RecoveredNpcServices {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct NpcServiceReceivers {
    outcomes: mpsc::Receiver<Arc<NpcServiceOutcome>>,
}
pub(super) struct NpcServiceSenders {
    outcomes: mpsc::SyncSender<Arc<NpcServiceOutcome>>,
}
pub(super) fn channels(capacity: usize) -> (NpcServiceSenders, NpcServiceReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(capacity);
    (
        NpcServiceSenders { outcomes },
        NpcServiceReceivers { outcomes: receiver },
    )
}
impl NpcServiceSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_npc_service_outcome() else {
                break;
            };
            if self.outcomes.try_send(outcome.clone()).is_err() {
                break;
            }
            kernel.take_npc_service_outcome();
        }
    }
}
impl NpcServiceReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredNpcServices) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn npc_service_outcomes(&self) -> &mpsc::Receiver<Arc<NpcServiceOutcome>> {
        &self.npc_services.outcomes
    }
}
