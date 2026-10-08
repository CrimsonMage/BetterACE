//! Exact immutable locomotion evidence survives pressure and owner recovery.
use super::*;
use bace_gameplay_api::locomotion::LocomotionOutcome;
#[derive(Default)]
pub struct RecoveredLocomotion {
    pub outcomes: Vec<Arc<LocomotionOutcome>>,
}
impl RecoveredLocomotion {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct LocomotionReceivers {
    outcomes: mpsc::Receiver<Arc<LocomotionOutcome>>,
}
pub(super) struct LocomotionSenders {
    outcomes: mpsc::SyncSender<Arc<LocomotionOutcome>>,
}
pub(super) fn channels(capacity: usize) -> (LocomotionSenders, LocomotionReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(capacity);
    (
        LocomotionSenders { outcomes },
        LocomotionReceivers { outcomes: receiver },
    )
}
impl LocomotionSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_locomotion_outcome() else {
                break;
            };
            if self.outcomes.try_send(outcome.clone()).is_err() {
                break;
            }
            kernel.take_locomotion_outcome();
        }
    }
}
impl LocomotionReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredLocomotion) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn locomotion_outcomes(&self) -> &mpsc::Receiver<Arc<LocomotionOutcome>> {
        &self.locomotion.outcomes
    }
}
