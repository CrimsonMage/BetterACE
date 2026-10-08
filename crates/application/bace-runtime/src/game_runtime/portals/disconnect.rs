//! A disconnected client cannot acknowledge a portal. Transfer its pending
//! completion to the existing detach owner instead of inventing ClientReady.
use super::*;
use bace_gameplay_api::CharacterBinding;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PortalDisconnectHandoff {
    key: SessionKey,
    correlation: u64,
    binding: CharacterBinding,
    operation: u64,
    view: bace_simulation::PortalAcceptedView,
}

impl PortalRuntime {
    fn can_handoff(&self, actor: EntityId, lifecycle: bool, observer_pending: bool) -> bool {
        lifecycle
            && self.transits.contains_key(&actor)
            && !self.holds_without_transit(actor)
            && !observer_pending
    }
    pub(super) fn holds_without_transit(&self, actor: EntityId) -> bool {
        self.deliveries.iter().any(|d| match &d.work {
            PortalDeliveryWork::Completed(c) => c.work.bindings.iter().any(|b| b.actor == actor),
            PortalDeliveryWork::Event(e) => match e {
                PortalServiceEvent::Hidden { actors, .. }
                | PortalServiceEvent::Teleported { actors, .. } => actors.contains(&actor),
                PortalServiceEvent::Materialized { actor: a, .. }
                | PortalServiceEvent::Linked { actor: a, .. }
                | PortalServiceEvent::Blocked { actor: a, .. }
                | PortalServiceEvent::AbortedAfterCommit { actor: a, .. } => *a == actor,
                PortalServiceEvent::Summoned { entity, .. }
                | PortalServiceEvent::Removed { entity } => *entity == actor,
            },
        }) || self.controls.values().any(|c| c.actor() == actor)
            || self.service.contains_actor(actor)
            || self
                .ticket
                .as_ref()
                .is_some_and(|t| t.participants.iter().any(|(id, _, _)| *id == actor))
            || self.publication_holds(actor)
    }

    fn complete_disconnect(
        &mut self,
        key: SessionKey,
        correlation: u64,
        binding: CharacterBinding,
        committed: bool,
    ) -> Result<(), String> {
        let Some(handoff) = self.detaching.get(&binding.actor) else {
            return Ok(());
        };
        if handoff.key != key || handoff.correlation != correlation || handoff.binding != binding {
            return Err("portal detach handoff correlation mismatch".into());
        }
        if committed {
            let transit = self
                .transits
                .get(&binding.actor)
                .ok_or("portal detach transit missing")?;
            if transit.operation != handoff.operation || transit.view != handoff.view {
                return Err("portal detach transit changed".into());
            }
            if self.death_materialization_watches.get(&binding.actor)
                == Some(&(transit.operation, transit.view.epoch))
            {
                self.materialized_receipts
                    .insert(binding.actor, (transit.operation, transit.view.epoch));
            }
            self.transits.remove(&binding.actor);
        }
        self.detaching.remove(&binding.actor);
        Ok(())
    }
}

impl GameRuntime {
    /// A waiver applies only to the readiness wait. All durable and delivery
    /// work must already have transferred to its exact downstream owner.
    pub(super) fn disconnected_portal_handoff(&self, key: SessionKey) -> bool {
        let Some(session) = self.sessions.get(&key) else {
            return false;
        };
        if !(session.terminated || self.draining) {
            return false;
        }
        let Some(loading) = &session.loading else {
            return false;
        };
        let actor = loading.loaded.binding.actor;
        self.portals
            .can_handoff(actor, true, self.observer_holds(actor))
    }

    /// Called only after the exact detach command enters the simulation queue.
    /// The transit remains retained until that command's correlated receipt.
    pub(in crate::game_runtime) fn begin_portal_disconnect(
        &mut self,
        key: SessionKey,
        correlation: u64,
    ) -> Result<(), String> {
        let Some(binding) = self.players.binding_for(key) else {
            return Err("portal detach binding missing".into());
        };
        if !self.portals.transits.contains_key(&binding.actor) {
            return Ok(());
        }
        if correlation == 0 || !self.disconnected_portal_handoff(key) {
            return Err("portal detach before delivery drain".into());
        }
        let transit = &self.portals.transits[&binding.actor];
        let handoff = PortalDisconnectHandoff {
            key,
            correlation,
            binding,
            operation: transit.operation,
            view: transit.view,
        };
        if self
            .portals
            .detaching
            .get(&binding.actor)
            .is_some_and(|old| *old != handoff)
        {
            return Err("portal detach already owned".into());
        }
        self.portals.detaching.insert(binding.actor, handoff);
        Ok(())
    }

    /// Success means the simulation's take_player_state already ran its trusted
    /// take_portal_links lifecycle cleanup. Failure keeps the original transit.
    pub(in crate::game_runtime) fn finish_portal_disconnect(
        &mut self,
        key: SessionKey,
        correlation: u64,
        binding: CharacterBinding,
        committed: bool,
    ) -> Result<(), String> {
        self.portals
            .complete_disconnect(key, correlation, binding, committed)
    }
}

#[cfg(test)]
mod tests;
