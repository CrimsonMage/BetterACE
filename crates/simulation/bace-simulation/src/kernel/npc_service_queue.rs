//! Trusted adapter completions retain exact correlated output under pressure.
use super::*;
use std::sync::Arc;
impl Kernel {
    pub(super) fn handle_npc_service(&mut self, command: crate::NpcServiceCommand) {
        let outcome = self.dispatch_npc_service(command);
        self.npc_service_outcomes.push_back(Arc::new(outcome));
    }
    pub fn peek_npc_service_outcome(&self) -> Option<&Arc<crate::NpcServiceOutcome>> {
        self.npc_service_outcomes.front()
    }
    pub fn take_npc_service_outcome(&mut self) -> Option<Arc<crate::NpcServiceOutcome>> {
        self.npc_service_outcomes.pop_front()
    }
    pub fn has_npc_service_work(&self) -> bool {
        !self.npc_service_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::NpcService(_)))
    }
}
