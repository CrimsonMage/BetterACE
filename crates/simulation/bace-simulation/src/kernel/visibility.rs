//! Immutable visibility evidence. Packet knowledge remains in bace-replication.
use super::*;
use bace_gameplay_api::visibility::{VisibilityCandidate, VisibilityRejection, VisibilitySnapshot};
impl Kernel {
    pub(super) fn handle_visibility_request(
        &mut self,
        request: bace_gameplay_api::visibility::VisibilityRequest,
    ) {
        let result = self.visibility_snapshot_with_buffer(
            request.binding,
            request.limit,
            request.candidates,
        );
        self.visibility_outcomes.push_back(std::sync::Arc::new(
            bace_gameplay_api::visibility::VisibilityOutcome {
                correlation: request.correlation,
                result,
            },
        ));
    }
    pub fn peek_visibility_outcome(
        &self,
    ) -> Option<&std::sync::Arc<bace_gameplay_api::visibility::VisibilityOutcome>> {
        self.visibility_outcomes.front()
    }
    pub fn take_visibility_outcome(
        &mut self,
    ) -> Option<std::sync::Arc<bace_gameplay_api::visibility::VisibilityOutcome>> {
        self.visibility_outcomes.pop_front()
    }
    pub fn has_visibility_work(&self) -> bool {
        !self.visibility_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::Visibility(_)))
    }
    pub fn visibility_snapshot(
        &mut self,
        binding: CharacterBinding,
        limit: usize,
    ) -> Result<VisibilitySnapshot, VisibilityRejection> {
        self.visibility_snapshot_with_buffer(binding, limit, Vec::new())
            .map_err(|(error, _)| error)
    }
    /// Reuse the returned snapshot buffer on the next query after reliable output
    /// admission. Rejecting a query returns the supplied buffer to its adapter.
    pub fn visibility_snapshot_with_buffer(
        &mut self,
        binding: CharacterBinding,
        limit: usize,
        mut candidates: Vec<VisibilityCandidate>,
    ) -> Result<VisibilitySnapshot, (VisibilityRejection, Vec<VisibilityCandidate>)> {
        let result = (|| {
            match self.characters.can_take_complete(binding) {
                Ok(()) | Err(CharacterRegistrationError::DurabilityPending) => {}
                _ => return Err(VisibilityRejection::NotBound),
            }
            let (_, state) = self
                .world
                .actor_state(binding.actor)
                .map_err(|_| VisibilityRejection::MissingActor)?;
            self.world
                .visibility_candidates(binding.actor, &mut candidates, limit)
                .map_err(|e| match e {
                    bace_world::VisibilityError::MissingActor => VisibilityRejection::MissingActor,
                    bace_world::VisibilityError::MissingCell => {
                        VisibilityRejection::MissingGeometry
                    }
                    bace_world::VisibilityError::Capacity => VisibilityRejection::Capacity,
                    _ => VisibilityRejection::InvalidState,
                })?;
            // Physics admission precedes durable login/self-entry. Other clients
            // may observe a player only after that player's entered fence.
            candidates.retain(|candidate| {
                !self
                    .world
                    .combatant(candidate.entity)
                    .is_some_and(|actor| actor.profile().player)
                    || self.characters.entered(candidate.entity)
            });
            Ok(state.epoch())
        })();
        match result {
            Ok(epoch) => Ok(VisibilitySnapshot {
                binding,
                observer_epoch: epoch,
                tick: self.tick,
                candidates,
            }),
            Err(error) => Err((error, candidates)),
        }
    }
}
