//! Preserve the exact inventory decision through bounded output pressure.
use super::*;
use bace_simulation::InventoryOutcome;
#[derive(Default)]
pub struct RecoveredInventory {
    pub outcomes: Vec<InventoryOutcome>,
}
impl RecoveredInventory {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct InventoryReceivers {
    outcomes: mpsc::Receiver<InventoryOutcome>,
}
pub(super) struct InventorySenders {
    outcomes: mpsc::SyncSender<InventoryOutcome>,
}
pub(super) fn channels(capacity: usize) -> (InventorySenders, InventoryReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(capacity);
    (
        InventorySenders { outcomes },
        InventoryReceivers { outcomes: receiver },
    )
}
impl InventorySenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.take_inventory_outcome() else {
                break;
            };
            if let Err(
                mpsc::TrySendError::Full(outcome) | mpsc::TrySendError::Disconnected(outcome),
            ) = self.outcomes.try_send(outcome)
            {
                assert!(
                    kernel.restore_inventory_outcome(outcome).is_ok(),
                    "single owner drained inventory output slot"
                );
                break;
            }
        }
    }
}
impl InventoryReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredInventory) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn inventory_outcomes(&self) -> &mpsc::Receiver<InventoryOutcome> {
        &self.inventory_output.outcomes
    }
}
