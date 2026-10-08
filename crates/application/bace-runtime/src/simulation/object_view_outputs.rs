//! Exact immutable object_view evidence survives pressure and owner recovery.
use super::*;
use bace_gameplay_api::visibility::ObjectViewOutcome;
#[derive(Default)]
pub struct RecoveredObjectView {
    pub outcomes: Vec<Arc<ObjectViewOutcome>>,
}
impl RecoveredObjectView {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct ObjectViewReceivers {
    outcomes: mpsc::Receiver<Arc<ObjectViewOutcome>>,
}
pub(super) struct ObjectViewSenders {
    outcomes: mpsc::SyncSender<Arc<ObjectViewOutcome>>,
}
pub(super) fn channels(capacity: usize) -> (ObjectViewSenders, ObjectViewReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(capacity);
    (
        ObjectViewSenders { outcomes },
        ObjectViewReceivers { outcomes: receiver },
    )
}
impl ObjectViewSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_object_view_outcome() else {
                break;
            };
            if self.outcomes.try_send(outcome.clone()).is_err() {
                break;
            }
            kernel.take_object_view_outcome();
        }
    }
}
impl ObjectViewReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredObjectView) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn object_view_outcomes(&self) -> &mpsc::Receiver<Arc<ObjectViewOutcome>> {
        &self.object_view.outcomes
    }
}
