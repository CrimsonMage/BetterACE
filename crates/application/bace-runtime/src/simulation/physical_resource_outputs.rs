//! Exact immutable physical_resource evidence survives pressure and owner recovery.
use super::*;
use bace_simulation::PhysicalResourceOutcome;
#[derive(Default)]
pub struct RecoveredPhysicalResource {
    pub outcomes: Vec<Arc<PhysicalResourceOutcome>>,
}
impl RecoveredPhysicalResource {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct PhysicalResourceReceivers {
    outcomes: mpsc::Receiver<Arc<PhysicalResourceOutcome>>,
}
pub(super) struct PhysicalResourceSenders {
    outcomes: mpsc::SyncSender<Arc<PhysicalResourceOutcome>>,
}
pub(super) fn channels(capacity: usize) -> (PhysicalResourceSenders, PhysicalResourceReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(capacity);
    (
        PhysicalResourceSenders { outcomes },
        PhysicalResourceReceivers { outcomes: receiver },
    )
}
impl PhysicalResourceSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_physical_resource_outcome() else {
                break;
            };
            if self.outcomes.try_send(outcome.clone()).is_err() {
                break;
            }
            kernel.take_physical_resource_outcome();
        }
    }
}
impl PhysicalResourceReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredPhysicalResource) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn physical_resource_outcomes(&self) -> &mpsc::Receiver<Arc<PhysicalResourceOutcome>> {
        &self.physical_resource.outcomes
    }
}
