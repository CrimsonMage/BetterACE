//! Exact immutable magic_resource evidence survives pressure and owner recovery.
use super::*;
use bace_simulation::MagicResourceOutcome;
#[derive(Default)]
pub struct RecoveredMagicResource {
    pub outcomes: Vec<Arc<MagicResourceOutcome>>,
}
impl RecoveredMagicResource {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct MagicResourceReceivers {
    outcomes: mpsc::Receiver<Arc<MagicResourceOutcome>>,
}
pub(super) struct MagicResourceSenders {
    outcomes: mpsc::SyncSender<Arc<MagicResourceOutcome>>,
}
pub(super) fn channels(capacity: usize) -> (MagicResourceSenders, MagicResourceReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(capacity);
    (
        MagicResourceSenders { outcomes },
        MagicResourceReceivers { outcomes: receiver },
    )
}
impl MagicResourceSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_magic_resource_outcome() else {
                break;
            };
            if self.outcomes.try_send(outcome.clone()).is_err() {
                break;
            }
            kernel.take_magic_resource_outcome();
        }
    }
}
impl MagicResourceReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredMagicResource) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn magic_resource_outcomes(&self) -> &mpsc::Receiver<Arc<MagicResourceOutcome>> {
        &self.magic_resource.outcomes
    }
}
