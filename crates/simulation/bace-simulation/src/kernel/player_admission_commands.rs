//! Trusted lifecycle ingress retains rejected preparations under backpressure.
use super::*;
impl Kernel {
    pub fn take_player_admission_outcome(&mut self) -> Option<crate::PlayerAdmissionOutcome> {
        self.player_admission_outcomes.pop_front()
    }
    /// A worker may restore an undelivered outcome into the slot it just drained.
    pub fn restore_player_admission_outcome(
        &mut self,
        outcome: crate::PlayerAdmissionOutcome,
    ) -> Result<(), crate::PlayerAdmissionOutcome> {
        if !self.player_admission_outcomes.is_empty() {
            return Err(outcome);
        }
        self.player_admission_outcomes.push_front(outcome);
        Ok(())
    }
    pub fn has_player_admission_work(&self) -> bool {
        self.has_player_detach_work()
            || !self.player_admission_outcomes.is_empty()
            || !self.player_entered_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::AdmitPlayer(_) | Command::PlayerEntered(_)))
    }
    pub(super) fn handle_player_admission(&mut self, request: crate::PlayerAdmissionRequest) {
        let binding = request.prepared.binding;
        let result = if request.valid_bounds() {
            self.admit_player(*request.prepared)
        } else {
            Err((crate::PlayerAdmissionError::Capacity, request.prepared))
        };
        self.player_admission_outcomes
            .push_back(crate::PlayerAdmissionOutcome {
                correlation: request.correlation,
                binding,
                result,
            });
    }
}

impl Kernel {
    pub fn peek_player_entered_outcome(&self) -> Option<&crate::PlayerEnteredOutcome> {
        self.player_entered_outcomes.front()
    }
    pub fn take_player_entered_outcome(&mut self) -> Option<crate::PlayerEnteredOutcome> {
        self.player_entered_outcomes.pop_front()
    }
    pub(super) fn handle_player_entered(&mut self, request: crate::PlayerEnteredRequest) {
        let result = if request.correlation == 0 {
            Err(bace_gameplay_api::UiError::Invalid)
        } else {
            self.character_entered(request.binding)
        };
        self.player_entered_outcomes
            .push_back(crate::PlayerEnteredOutcome {
                correlation: request.correlation,
                binding: request.binding,
                result,
            });
    }
}
