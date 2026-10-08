//! A rejected admission retains its whole prepared owner under output pressure.
use super::*;
use bace_simulation::PlayerAdmissionOutcome;
#[derive(Default)]
pub struct RecoveredPlayerAdmissions {
    pub outcomes: Vec<PlayerAdmissionOutcome>,
    pub entered: Vec<bace_simulation::PlayerEnteredOutcome>,
    pub detached: Vec<bace_simulation::PlayerDetachOutcome>,
}
impl RecoveredPlayerAdmissions {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty() || !self.entered.is_empty() || !self.detached.is_empty()
    }
}
pub(super) struct PlayerAdmissionReceivers {
    outcomes: mpsc::Receiver<PlayerAdmissionOutcome>,
    entered: mpsc::Receiver<bace_simulation::PlayerEnteredOutcome>,
    detached: mpsc::Receiver<bace_simulation::PlayerDetachOutcome>,
}
pub(super) struct PlayerAdmissionSenders {
    outcomes: mpsc::SyncSender<PlayerAdmissionOutcome>,
    entered: mpsc::SyncSender<bace_simulation::PlayerEnteredOutcome>,
    detached: mpsc::SyncSender<bace_simulation::PlayerDetachOutcome>,
}
pub(super) fn channels(_capacity: usize) -> (PlayerAdmissionSenders, PlayerAdmissionReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(1);
    let (entered, entered_receiver) = mpsc::sync_channel(1);
    let (detached, detached_receiver) = mpsc::sync_channel(1);
    (
        PlayerAdmissionSenders {
            outcomes,
            entered,
            detached,
        },
        PlayerAdmissionReceivers {
            outcomes: receiver,
            entered: entered_receiver,
            detached: detached_receiver,
        },
    )
}
impl PlayerAdmissionSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        if let Some(outcome) = kernel.take_player_detach_outcome()
            && let Err(
                mpsc::TrySendError::Full(outcome) | mpsc::TrySendError::Disconnected(outcome),
            ) = self.detached.try_send(outcome)
        {
            assert!(
                kernel.restore_player_detach_outcome(outcome).is_ok(),
                "single-owner drained detach slot"
            );
        }
        if let Some(outcome) = kernel.peek_player_entered_outcome()
            && self.entered.try_send(*outcome).is_ok()
        {
            kernel.take_player_entered_outcome();
        }
        for _ in 0..budget {
            let Some(outcome) = kernel.take_player_admission_outcome() else {
                break;
            };
            if let Err(
                mpsc::TrySendError::Full(outcome) | mpsc::TrySendError::Disconnected(outcome),
            ) = self.outcomes.try_send(outcome)
            {
                assert!(
                    kernel.restore_player_admission_outcome(outcome).is_ok(),
                    "single-owner drained admission slot"
                );
                break;
            }
        }
    }
}
impl PlayerAdmissionReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredPlayerAdmissions) {
        output.outcomes.extend(self.outcomes.try_iter());
        output.entered.extend(self.entered.try_iter());
        output.detached.extend(self.detached.try_iter());
    }
}
impl SimulationWorker {
    pub fn player_detach_outcomes(&self) -> &mpsc::Receiver<bace_simulation::PlayerDetachOutcome> {
        &self.player_admissions.detached
    }
    pub fn player_entered_outcomes(
        &self,
    ) -> &mpsc::Receiver<bace_simulation::PlayerEnteredOutcome> {
        &self.player_admissions.entered
    }
    pub fn player_admission_outcomes(&self) -> &mpsc::Receiver<PlayerAdmissionOutcome> {
        &self.player_admissions.outcomes
    }
}
