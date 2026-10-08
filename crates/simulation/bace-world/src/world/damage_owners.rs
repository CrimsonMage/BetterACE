//! Immutable bounded damage-credit ownership. Attacks freeze the returned ID at
//! admission; delayed effects must not re-resolve attribution after retirement.
use super::*;
impl World {
    /// Commit a prepared summoned actor and its immutable credit relation in one
    /// owner mutation. Failed admission returns the full actor for recovery.
    pub fn insert_damage_owned(
        &mut self,
        actor: Actor,
        owner: EntityId,
    ) -> Result<(), (WorldError, Box<Actor>)> {
        let credited = self.damage_owner(owner);
        let failure = if owner.0 == 0
            || !self.actors.contains_key(&owner)
            || actor.id == credited
            || self.damage_owners.len() >= 4096
        {
            Some(WorldError::InvalidDamageOwner)
        } else {
            self.validate_actor(&actor).err()
        };
        if let Some(error) = failure {
            return Err((error, Box::new(actor)));
        }
        self.damage_owners.insert(actor.id, credited);
        self.actors.insert(actor.id, actor);
        Ok(())
    }
    pub fn register_damage_owner(
        &mut self,
        source: EntityId,
        owner: EntityId,
    ) -> Result<(), WorldError> {
        if source.0 == 0
            || owner.0 == 0
            || source == owner
            || !self.actors.contains_key(&source)
            || !self.actors.contains_key(&owner)
        {
            return Err(WorldError::InvalidDamageOwner);
        }
        let credited = self.damage_owner(owner);
        if credited == source {
            return Err(WorldError::InvalidDamageOwner);
        }
        if let Some(old) = self.damage_owners.get(&source) {
            return if *old == credited {
                Ok(())
            } else {
                Err(WorldError::InvalidDamageOwner)
            };
        }
        if self.damage_owners.values().any(|id| *id == source) {
            return Err(WorldError::InvalidDamageOwner);
        }
        if self.damage_owners.len() >= 4096 {
            return Err(WorldError::InvalidDamageOwner);
        }
        // Store the already resolved owner, avoiding mutable parent chains.
        self.damage_owners.insert(source, credited);
        Ok(())
    }
    pub fn damage_owner(&self, source: EntityId) -> EntityId {
        self.damage_owners.get(&source).copied().unwrap_or(source)
    }
    pub(super) fn retire_damage_owner(&mut self, source: EntityId) {
        if !self.actors.contains_key(&source)
            && !self.projectiles.values().any(|p| p.source == source)
        {
            self.damage_owners.remove(&source);
        }
    }
}
