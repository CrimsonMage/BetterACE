//! Exact immutable visibility evidence survives pressure and owner recovery.
use super::*;
use bace_gameplay_api::visibility::VisibilityOutcome;
#[derive(Default)]
pub struct RecoveredVisibility {
    pub outcomes: Vec<Arc<VisibilityOutcome>>,
}
impl RecoveredVisibility {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct VisibilityReceivers {
    outcomes: mpsc::Receiver<Arc<VisibilityOutcome>>,
}
pub(super) struct VisibilitySenders {
    outcomes: mpsc::SyncSender<Arc<VisibilityOutcome>>,
}
pub(super) fn channels(capacity: usize) -> (VisibilitySenders, VisibilityReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(capacity);
    (
        VisibilitySenders { outcomes },
        VisibilityReceivers { outcomes: receiver },
    )
}
impl VisibilitySenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_visibility_outcome() else {
                break;
            };
            if self.outcomes.try_send(outcome.clone()).is_err() {
                break;
            }
            kernel.take_visibility_outcome();
        }
    }
}
impl VisibilityReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredVisibility) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn visibility_outcomes(&self) -> &mpsc::Receiver<Arc<VisibilityOutcome>> {
        &self.visibility.outcomes
    }
}
