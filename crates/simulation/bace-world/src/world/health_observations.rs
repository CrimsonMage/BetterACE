//! Source Creature.selectedTargets and OnHealthUpdate. Notifications capture the
//! accepted intermediate pool and subscription, never a later tick's final state.
use super::*;
use bace_gameplay_api::ActionContext;
use std::collections::VecDeque;
const CAPACITY: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HealthObservation {
    pub context: ActionContext,
    pub target: EntityId,
    pub current: u32,
    pub maximum: u32,
}
#[derive(Default)]
pub(super) struct HealthObservations {
    subscriptions: BTreeMap<EntityId, BTreeMap<EntityId, ActionContext>>,
    count: usize,
    pending: VecDeque<HealthObservation>,
}
impl World {
    pub fn validate_health_subscription(
        &self,
        observer: EntityId,
        target: Option<EntityId>,
    ) -> Result<(), WorldError> {
        if !self.actors.contains_key(&observer)
            || target.is_some_and(|id| !self.combatants.contains_key(&id))
        {
            return Err(WorldError::MissingActor);
        }
        let exists = self
            .health_observations
            .subscriptions
            .values()
            .any(|s| s.contains_key(&observer));
        if target.is_some() && !exists && self.health_observations.count == CAPACITY {
            return Err(WorldError::HealthBackpressure);
        }
        Ok(())
    }
    pub fn set_health_subscription(
        &mut self,
        context: ActionContext,
        target: Option<EntityId>,
    ) -> Result<(), WorldError> {
        self.validate_health_subscription(context.actor, target)?;
        self.clear_health_subscription(context.actor);
        if let Some(target) = target {
            self.health_observations
                .subscriptions
                .entry(target)
                .or_default()
                .insert(context.actor, context);
            self.health_observations.count += 1;
        }
        Ok(())
    }
    pub(super) fn retire_health_subscriptions(&mut self, actor: EntityId) {
        self.clear_health_subscription(actor);
        if let Some(entries) = self.health_observations.subscriptions.remove(&actor) {
            self.health_observations.count -= entries.len();
        }
    }
    pub fn clear_health_subscription(&mut self, observer: EntityId) {
        self.health_observations.subscriptions.retain(|_, entries| {
            if entries.remove(&observer).is_some() {
                self.health_observations.count -= 1;
            }
            !entries.is_empty()
        });
    }
    pub fn validate_health_observations(
        &self,
        targets: impl IntoIterator<Item = EntityId>,
    ) -> Result<(), WorldError> {
        let mut count = self.health_observations.pending.len();
        for target in targets {
            count = count
                .checked_add(
                    self.health_observations
                        .subscriptions
                        .get(&target)
                        .map_or(0, BTreeMap::len),
                )
                .ok_or(WorldError::HealthBackpressure)?;
            if count > CAPACITY {
                return Err(WorldError::HealthBackpressure);
            }
        }
        Ok(())
    }
    pub fn pending_health_observation(&self) -> Option<&HealthObservation> {
        self.health_observations.pending.front()
    }
    pub fn take_health_observation(&mut self) -> Option<HealthObservation> {
        self.health_observations.pending.pop_front()
    }
    pub fn health_observation_pending_for(&self, actor: EntityId) -> bool {
        self.health_observations
            .pending
            .iter()
            .any(|e| e.context.actor == actor)
    }
    pub(super) fn health_observation_mentions(&self, actor: EntityId) -> bool {
        self.health_observations
            .pending
            .iter()
            .any(|e| e.context.actor == actor || e.target == actor)
    }
    pub(super) fn publish_health_observation(&mut self, target: EntityId) {
        let Some(observers) = self.health_observations.subscriptions.get(&target) else {
            return;
        };
        let state = self.combatants.get(&target).expect("accepted health owner");
        assert!(
            self.health_observations.pending.len() + observers.len() <= CAPACITY,
            "health output preflight"
        );
        for context in observers.values() {
            self.health_observations
                .pending
                .push_back(HealthObservation {
                    context: *context,
                    target,
                    current: state.health(),
                    maximum: state.profile().maximum_health,
                });
        }
    }
    /// Accepted damage must enter here rather than bypassing the observation
    /// preflight through the low-level Combatant maintenance accessor.
    pub fn damage_from(
        &mut self,
        target: EntityId,
        source: EntityId,
        amount: u32,
    ) -> Result<u32, WorldError> {
        if source.0 == 0 {
            return Err(WorldError::InvalidVital);
        }
        self.validate_health_observations([target])?;
        if self.vital_reserved(target, bace_entity::EntityVital::Health) {
            return Err(WorldError::VitalReserved);
        }
        let applied = self
            .combatants
            .get_mut(&target)
            .ok_or(WorldError::MissingActor)?
            .damage_from(source, amount)
            .map_err(|_| WorldError::InvalidVital)?;
        if applied != 0 {
            self.publish_health_observation(target);
        }
        Ok(applied)
    }
}
#[cfg(test)]
mod tests;
