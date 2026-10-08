//! Immutable movement acceptance evidence has one bounded retained output slot.
use super::*;
use bace_gameplay_api::locomotion::{LocomotionCommand, LocomotionOutcome};
impl Kernel {
    pub(super) fn handle_locomotion_request(&mut self, command: LocomotionCommand) {
        let outcome = self.apply_locomotion(command);
        self.locomotion_outcomes
            .push_back(std::sync::Arc::new(outcome));
    }
    pub fn peek_locomotion_outcome(&self) -> Option<&std::sync::Arc<LocomotionOutcome>> {
        self.locomotion_outcomes.front()
    }
    pub fn take_locomotion_outcome(&mut self) -> Option<std::sync::Arc<LocomotionOutcome>> {
        self.locomotion_outcomes.pop_front()
    }
    pub fn has_locomotion_work(&self) -> bool {
        !self.locomotion_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::Locomotion(_)))
    }
}
