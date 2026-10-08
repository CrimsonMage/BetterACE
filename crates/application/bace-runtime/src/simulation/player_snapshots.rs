//! One online immutable player capture in transit; stalled consumers retain it.
use super::*;
use bace_simulation::PlayerSnapshotOutcome;
#[derive(Default)]
pub struct RecoveredPlayerSnapshots {
    pub outcomes: Vec<PlayerSnapshotOutcome>,
}
impl RecoveredPlayerSnapshots {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct PlayerSnapshotReceivers {
    outcomes: mpsc::Receiver<PlayerSnapshotOutcome>,
}
pub(super) struct PlayerSnapshotSenders {
    outcomes: mpsc::SyncSender<PlayerSnapshotOutcome>,
}
pub(super) fn channels(_capacity: usize) -> (PlayerSnapshotSenders, PlayerSnapshotReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(1);
    (
        PlayerSnapshotSenders { outcomes },
        PlayerSnapshotReceivers { outcomes: receiver },
    )
}
impl PlayerSnapshotSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_player_snapshot_outcome() else {
                break;
            };
            if self.outcomes.try_send(outcome.clone()).is_err() {
                break;
            }
            kernel.take_player_snapshot_outcome();
        }
    }
}
impl PlayerSnapshotReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredPlayerSnapshots) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn player_snapshot_outcomes(&self) -> &mpsc::Receiver<PlayerSnapshotOutcome> {
        &self.player_snapshots.outcomes
    }
}
