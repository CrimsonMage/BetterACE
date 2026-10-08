//! Transfer only an exact saved aggregate and its complete owned item closure.
use super::*;
impl Kernel {
    pub fn take_player_detach_outcome(&mut self) -> Option<crate::PlayerDetachOutcome> {
        self.player_detach_outcomes.pop_front()
    }
    pub fn restore_player_detach_outcome(
        &mut self,
        outcome: crate::PlayerDetachOutcome,
    ) -> Result<(), Box<crate::PlayerDetachOutcome>> {
        if !self.player_detach_outcomes.is_empty() {
            return Err(Box::new(outcome));
        }
        self.player_detach_outcomes.push_front(outcome);
        Ok(())
    }
    pub fn has_player_detach_work(&self) -> bool {
        !self.player_detach_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::DetachPlayer(_)))
    }
    pub(super) fn handle_player_detach(&mut self, request: crate::PlayerDetachRequest) {
        let result = self.detach_player(&request);
        self.player_detach_outcomes
            .push_back(crate::PlayerDetachOutcome {
                correlation: request.correlation,
                binding: request.binding,
                result,
            });
    }
    pub fn detach_player(
        &mut self,
        request: &crate::PlayerDetachRequest,
    ) -> Result<crate::DetachedPlayer, CharacterRegistrationError> {
        use CharacterRegistrationError as E;
        if request.correlation == 0 || request.expected_items.len() > 1023 {
            return Err(E::Capacity);
        }
        if self.pk_timer_active(request.binding.actor) {
            return Err(E::DurabilityPending);
        }
        // Synchronize registry timers, age and accepted physics before comparing
        // the durable receipt fences. A late mutation requires another save.
        let snapshot = self.read_player_snapshot(request.binding)?;
        if if request.capture_final {
            snapshot.character().progression().revision() < request.expected_revision
        } else {
            snapshot.character().progression().revision() != request.expected_revision
        } {
            return Err(E::DurabilityPending);
        }
        let mut expected = request.expected_items.clone();
        expected.sort_unstable();
        if expected.windows(2).any(|p| p[0].0 == p[1].0) {
            return Err(E::OwnershipMismatch);
        }
        let actual: Vec<_> = snapshot
            .items()
            .iter()
            .map(|i| (i.id, i.revision))
            .collect();
        if if request.capture_final {
            expected.len() != actual.len()
                || expected
                    .iter()
                    .zip(&actual)
                    .any(|(e, a)| e.0 != a.0 || e.1 > a.1)
        } else {
            expected != actual
        } {
            return Err(E::DurabilityPending);
        }
        // An accepted logout starts the same durable device release as pet
        // expiry. Keep player ownership until that exact inventory receipt
        // retires the live pet; repeated detach requests reuse its operation.
        if self
            .request_pet_stow(request.binding.actor)
            .map_err(|_| E::DurabilityPending)?
            .is_some()
        {
            return Err(E::DurabilityPending);
        }
        self.inventory
            .preflight_player_detach(request.binding.actor)
            .map_err(|_| E::DurabilityPending)?;
        // take_player_state preflights every remaining domain and performs no
        // fallible mutation after that preflight, on this same owner turn.
        let state = self.take_player_state(request.binding)?;
        self.player_deaths
            .consent_grants
            .remove(&request.binding.actor);
        let (items, containers) = self.inventory.detach_player(request.binding.actor);
        self.pets.owners.remove(&request.binding.actor);
        self.combat.retire_actor(request.binding.actor);
        let actor = self
            .world
            .remove(request.binding.actor)
            .expect("accepted snapshot and unreserved world owner");
        Ok(crate::DetachedPlayer {
            snapshot: std::sync::Arc::new(snapshot),
            state,
            actor,
            items,
            containers,
        })
    }
}
