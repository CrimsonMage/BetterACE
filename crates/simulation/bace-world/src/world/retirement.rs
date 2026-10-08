//! A bounded single-owner lifecycle hold freezes advancement, never creates a
//! replacement body. Dynamic collision membership and accepted state remain.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldRetirementHold {
    pub actor: EntityId,
    pub epoch: u16,
    pub operation: u64,
}
impl WorldRetirementHold {
    pub fn vital_token(self) -> VitalReservationToken {
        VitalReservationToken {
            domain: VitalReservationDomain::NpcRetirement,
            operation: self.operation,
        }
    }
}
impl World {
    pub fn retirement_held(&self, actor: EntityId) -> bool {
        self.retirement_holds.contains_key(&actor)
    }
    pub fn retirement_hold(&self, actor: EntityId) -> Option<WorldRetirementHold> {
        self.retirement_holds.get(&actor).copied()
    }
    pub fn hold_retirement(
        &mut self,
        actor: EntityId,
        operation: u64,
    ) -> Result<WorldRetirementHold, WorldError> {
        let epoch = self.body(actor)?.accepted().epoch();
        let hold = WorldRetirementHold {
            actor,
            epoch,
            operation,
        };
        if operation == 0
            || self
                .combatants
                .get(&actor)
                .is_some_and(|c| c.profile().player)
        {
            return Err(WorldError::InvalidVital);
        }
        if let Some(current) = self.retirement_holds.get(&actor) {
            return if *current == hold {
                Ok(hold)
            } else {
                Err(WorldError::VitalReserved)
            };
        }
        if self.retirement_holds.len() >= 4096
            || self
                .retirement_holds
                .values()
                .any(|v| v.operation == operation)
            || self.portal_transit.contains_key(&actor)
            || !self.can_retire_actor_motion(actor)
        {
            return Err(WorldError::InvalidMotion);
        }
        if self.has_reserved_vitals_except(actor, hold.vital_token()) {
            return Err(WorldError::VitalReserved);
        }
        let resources: Vec<_> = [
            bace_entity::EntityVital::Health,
            bace_entity::EntityVital::Stamina,
            bace_entity::EntityVital::Mana,
        ]
        .into_iter()
        .filter_map(|v| self.vital(actor, v).ok().map(|_| (actor, v)))
        .collect();
        if !resources.is_empty() {
            self.reserve_vitals(&resources, hold.vital_token())?;
        }
        self.retirement_holds.insert(actor, hold);
        Ok(hold)
    }
    pub fn release_retirement(&mut self, hold: WorldRetirementHold) -> Result<(), WorldError> {
        if self.retirement_holds.get(&hold.actor) != Some(&hold)
            || self.body(hold.actor)?.accepted().epoch() != hold.epoch
        {
            return Err(WorldError::InvalidMotion);
        }
        self.retirement_holds.remove(&hold.actor);
        self.release_vitals(hold.vital_token());
        Ok(())
    }
    pub fn remove_retired(&mut self, hold: WorldRetirementHold) -> Result<Actor, WorldError> {
        if self.retirement_holds.get(&hold.actor) != Some(&hold)
            || self.body(hold.actor)?.accepted().epoch() != hold.epoch
            || !self.can_retire_actor_motion(hold.actor)
        {
            return Err(WorldError::InvalidMotion);
        }
        if self.has_reserved_vitals_except(hold.actor, hold.vital_token()) {
            return Err(WorldError::VitalReserved);
        }
        self.retirement_holds.remove(&hold.actor);
        self.release_vitals(hold.vital_token());
        self.remove(hold.actor).ok_or(WorldError::MissingActor)
    }
}
