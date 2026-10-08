//! Correlated immutable rendering inputs from the single simulation owner.
use super::*;
use bace_gameplay_api::visibility::{
    ObjectViewOutcome, ObjectViewRejection, ObjectViewRequest, ObjectViewSnapshot,
};
impl Kernel {
    pub fn object_view_snapshot(
        &self,
        binding: CharacterBinding,
        entities: &[EntityId],
    ) -> Result<ObjectViewSnapshot, ObjectViewRejection> {
        match self.characters.can_take_complete(binding) {
            Ok(()) | Err(CharacterRegistrationError::DurabilityPending) => {}
            _ => return Err(ObjectViewRejection::NotBound),
        }
        if entities.len() > 4097
            || entities.iter().any(|id| id.0 == 0)
            || entities.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(ObjectViewRejection::Capacity);
        }
        if self.world.is_in_portal_transit(binding.actor) {
            return Err(ObjectViewRejection::InTransit);
        }
        let observer_epoch = self
            .world
            .actor_state(binding.actor)
            .map_err(|_| ObjectViewRejection::MissingActor)?
            .1
            .epoch();
        let views = entities
            .iter()
            .map(|id| (*id, self.world.accepted_object_view(*id)))
            .collect();
        Ok(ObjectViewSnapshot {
            binding,
            observer_epoch,
            tick: self.tick,
            views,
        })
    }
    pub(super) fn handle_object_view_request(&mut self, request: ObjectViewRequest) {
        let result = self.object_view_snapshot(request.binding, &request.entities);
        self.object_view_outcomes
            .push_back(std::sync::Arc::new(ObjectViewOutcome {
                correlation: request.correlation,
                result,
            }));
    }
    pub fn peek_object_view_outcome(&self) -> Option<&std::sync::Arc<ObjectViewOutcome>> {
        self.object_view_outcomes.front()
    }
    pub fn take_object_view_outcome(&mut self) -> Option<std::sync::Arc<ObjectViewOutcome>> {
        self.object_view_outcomes.pop_front()
    }
    pub fn has_object_view_work(&self) -> bool {
        !self.object_view_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::ObjectView(_)))
    }
}
