//! Resource ownership while durable component/portal transactions are pending.
//! Tokens authorize mutation, never report a database commit or expire by clock.
use super::{World, WorldError};
use bace_entity::{EntityVital, VitalMutation, VitalMutationResult};
use bace_types::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VitalReservationDomain {
    SpellComponents,
    PlayerDeath,
    Experience,
    Crafting,
    Portal,
    NpcRetirement,
    Equipment,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VitalReservationToken {
    pub domain: VitalReservationDomain,
    pub operation: u64,
}
impl World {
    pub fn validate_vital_maxima(
        &self,
        actor: EntityId,
        maxima: [u32; 3],
    ) -> Result<(), WorldError> {
        if self.has_reserved_vitals(actor) {
            return Err(WorldError::VitalReserved);
        }
        self.combatants
            .get(&actor)
            .ok_or(WorldError::MissingActor)?
            .validate_vital_maxima(maxima)
            .map_err(|_| WorldError::InvalidVital)
    }
    pub fn replace_vital_maxima(
        &mut self,
        actor: EntityId,
        maxima: [u32; 3],
    ) -> Result<(), WorldError> {
        self.validate_vital_maxima(actor, maxima)?;
        self.combatants
            .get_mut(&actor)
            .ok_or(WorldError::MissingActor)?
            .replace_vital_maxima(maxima)
            .map_err(|_| WorldError::InvalidVital)
    }
    pub fn reserve_vitals(
        &mut self,
        resources: &[(EntityId, EntityVital)],
        token: VitalReservationToken,
    ) -> Result<(), WorldError> {
        if token.operation == 0 || resources.is_empty() || resources.len() > 64 {
            return Err(WorldError::InvalidVital);
        }
        for (index, resource) in resources.iter().enumerate() {
            // Ordinary components/portals cannot freeze Health. Death, shared
            // rewards and durable NPC retirement have explicit whole-pool owners.
            if (!matches!(
                token.domain,
                VitalReservationDomain::PlayerDeath
                    | VitalReservationDomain::Experience
                    | VitalReservationDomain::Crafting
                    | VitalReservationDomain::NpcRetirement
                    | VitalReservationDomain::Equipment
            ) && resource.1 != EntityVital::Mana
                && !(resource.1 == EntityVital::Stamina
                    && token.domain == VitalReservationDomain::Portal))
                || resources[..index].contains(resource)
            {
                return Err(WorldError::InvalidVital);
            }
            self.vital(resource.0, resource.1)?;
            if self
                .vital_reservations
                .get(resource)
                .is_some_and(|old| *old != token)
            {
                return Err(WorldError::VitalReserved);
            }
        }
        // Reusing a live token is an exact retry, never expansion or shrinking.
        let existing: Vec<_> = self
            .vital_reservations
            .iter()
            .filter_map(|(key, value)| (*value == token).then_some(*key))
            .collect();
        if !existing.is_empty() {
            return if existing.len() == resources.len()
                && existing.iter().all(|r| resources.contains(r))
            {
                Ok(())
            } else {
                Err(WorldError::VitalReserved)
            };
        }
        if self.vital_reservations.len() + resources.len() > 4096 {
            return Err(WorldError::VitalReserved);
        }
        for resource in resources {
            self.vital_reservations.insert(*resource, token);
        }
        Ok(())
    }
    pub fn release_vitals(&mut self, token: VitalReservationToken) {
        self.vital_reservations.retain(|_, owner| *owner != token);
    }
    pub fn has_reserved_vitals(&self, actor: EntityId) -> bool {
        self.vital_reservations
            .range((actor, EntityVital::Health)..=(actor, EntityVital::Mana))
            .next()
            .is_some()
    }
    pub fn has_reserved_vitals_except(
        &self,
        actor: EntityId,
        token: VitalReservationToken,
    ) -> bool {
        self.vital_reservations
            .range((actor, EntityVital::Health)..=(actor, EntityVital::Mana))
            .any(|(_, owner)| *owner != token)
    }
    pub fn vital_reserved(&self, actor: EntityId, vital: EntityVital) -> bool {
        self.vital_reservations.contains_key(&(actor, vital))
    }
    pub fn has_vital_reservations(&self) -> bool {
        !self.vital_reservations.is_empty()
    }
    pub(super) fn validate_vital_reservations(
        &self,
        changes: &[VitalMutation],
        token: Option<VitalReservationToken>,
    ) -> Result<(), WorldError> {
        if token.is_some_and(|token| {
            token.operation == 0
                || !self
                    .vital_reservations
                    .values()
                    .any(|owner| *owner == token)
        }) {
            return Err(WorldError::VitalReserved);
        }
        for change in changes {
            if self
                .vital_reservations
                .get(&(change.actor, change.vital))
                .is_some_and(|owner| Some(*owner) != token)
            {
                return Err(WorldError::VitalReserved);
            }
        }
        Ok(())
    }
    pub fn validate_vital_batch_reserved(
        &self,
        changes: &[VitalMutation],
        damage_source: Option<EntityId>,
        token: VitalReservationToken,
    ) -> Result<(), WorldError> {
        self.validate_vital_reservations(changes, Some(token))?;
        self.validate_vital_batch_values(changes, damage_source)
    }
    pub fn apply_vital_batch_reserved(
        &mut self,
        changes: &[VitalMutation],
        damage_source: Option<EntityId>,
        token: VitalReservationToken,
    ) -> Result<Vec<VitalMutationResult>, WorldError> {
        self.validate_vital_batch_reserved(changes, damage_source, token)?;
        Ok(self.apply_validated_vital_batch(changes, damage_source))
    }
}
