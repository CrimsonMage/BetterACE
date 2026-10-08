//! Player death keeps the player's body and admits a separate ethereal corpse.
use super::*;
impl World {
    /// Admit the copied world roots of a committed player NoCorpse death while
    /// retaining the dead player body for its later respawn. The caller owns
    /// the durable item graph and presents only its world roots here.
    pub fn preflight_player_death_world_roots(
        &self,
        source: EntityId,
        roots: &[Actor],
    ) -> Result<(), WorldError> {
        if roots.len() > 1024
            || self
                .anchors
                .len()
                .checked_add(roots.len())
                .is_none_or(|count| count > 4096)
        {
            return Err(WorldError::DuplicateActor);
        }
        let player = self.actors.get(&source).ok_or(WorldError::MissingActor)?;
        if self
            .combatants
            .get(&source)
            .is_none_or(|combatant| !combatant.profile().player || combatant.health() != 0)
        {
            return Err(WorldError::LivingActor);
        }
        let pose = player.body.accepted();
        let mut ids = BTreeSet::new();
        for actor in roots {
            if actor.id.0 == 0
                || actor.id.0 == u32::MAX
                || actor.id == source
                || !ids.insert(actor.id)
                || self.contains_identity(actor.id)
                || actor.cell != player.cell
                || actor.body.accepted().position() != pose.position()
                || (actor.body.accepted().heading_radians() - pose.heading_radians()).abs() > 0.0001
            {
                return Err(WorldError::DuplicateActor);
            }
            if let Some(shape) = actor.body.collision_shape() {
                self.geometry
                    .as_ref()
                    .ok_or(WorldError::MissingGeometry)?
                    .validate_placement(
                        actor.cell.0,
                        actor.body.accepted().position(),
                        shape,
                        &[],
                        actor.id.0,
                    )
                    .map_err(PhysicsError::from)?;
            } else {
                actor.body.validate_placement(self.scene(actor.cell)?)?;
            }
        }
        Ok(())
    }

    pub fn adopt_player_death_world_roots(
        &mut self,
        source: EntityId,
        roots: Vec<Actor>,
    ) -> Result<(), (WorldError, Vec<Actor>)> {
        if let Err(error) = self.preflight_player_death_world_roots(source, &roots) {
            return Err((error, roots));
        }
        for actor in roots {
            self.anchors.insert(actor.id);
            self.actors.insert(actor.id, actor);
        }
        self.visibility.invalidate();
        Ok(())
    }

    /// Corpse inventory roots become ethereal world-item anchors, validated
    /// against the same geometry owner. The caller owns the exact item graph.
    pub fn validate_corpse_spills(
        &self,
        corpse: EntityId,
        actors: &[Actor],
    ) -> Result<(), WorldError> {
        let source = self.corpses.get(&corpse).ok_or(WorldError::MissingActor)?;
        if actors.is_empty()
            || actors.len() > 1023
            || self
                .anchors
                .len()
                .checked_add(actors.len())
                .is_none_or(|n| n > 4096)
        {
            return Err(WorldError::DuplicateActor);
        }
        let mut seen = std::collections::BTreeSet::new();
        for actor in actors {
            if !seen.insert(actor.id)
                || !source.items.contains(&actor.id)
                || self.actors.contains_key(&actor.id)
                || self.doors.contains_key(&actor.id)
                || self.projectiles.contains_key(&actor.id)
            {
                return Err(WorldError::DuplicateActor);
            }
            if let Some(shape) = actor.body.collision_shape() {
                self.geometry
                    .as_ref()
                    .ok_or(WorldError::MissingGeometry)?
                    .validate_placement(
                        actor.cell.0,
                        actor.body.accepted().position(),
                        shape,
                        &[],
                        actor.id.0,
                    )
                    .map_err(PhysicsError::from)?;
            } else {
                actor.body.validate_placement(self.scene(actor.cell)?)?;
            }
        }
        Ok(())
    }
    pub fn admit_corpse_spills(
        &mut self,
        corpse: EntityId,
        actors: Vec<Actor>,
    ) -> Result<(), (WorldError, Vec<Actor>)> {
        if let Err(error) = self.validate_corpse_spills(corpse, &actors) {
            return Err((error, actors));
        }
        for actor in actors {
            self.anchors.insert(actor.id);
            self.actors.insert(actor.id, actor);
        }
        self.visibility.invalidate();
        Ok(())
    }
    /// Stop accepted locomotion after death while retaining gravity and collision.
    pub fn stop_player_death_motion(&mut self, actor: EntityId) -> Result<(), WorldError> {
        if self
            .combatants
            .get(&actor)
            .is_none_or(|c| !c.profile().player || c.health() != 0)
        {
            return Err(WorldError::LivingActor);
        }
        self.actors
            .get_mut(&actor)
            .ok_or(WorldError::MissingActor)?
            .body
            .stop_motion();
        Ok(())
    }

    pub fn validate_player_corpse(
        &self,
        source: EntityId,
        corpse: &Actor,
    ) -> Result<(), WorldError> {
        if corpse.id.0 == 0 || self.contains_identity(corpse.id) || self.anchors.len() >= 4096 {
            return Err(WorldError::DuplicateActor);
        }
        let player = self.actors.get(&source).ok_or(WorldError::MissingActor)?;
        if self
            .combatants
            .get(&source)
            .is_none_or(|c| !c.profile().player || c.health() != 0)
            || corpse.cell != player.cell
            || corpse.body.accepted().position() != player.body.accepted().position()
        {
            return Err(WorldError::LivingActor);
        }
        if let Some(shape) = corpse.body.collision_shape() {
            self.geometry
                .as_ref()
                .ok_or(WorldError::MissingGeometry)?
                .validate_placement(
                    corpse.cell.0,
                    corpse.body.accepted().position(),
                    shape,
                    &[],
                    corpse.id.0,
                )
                .map_err(PhysicsError::from)?;
        } else {
            corpse.body.validate_placement(self.scene(corpse.cell)?)?;
        }
        Ok(())
    }
    pub fn insert_player_corpse(
        &mut self,
        source: EntityId,
        corpse: Actor,
        state: CorpseState,
    ) -> Result<(), (WorldError, Box<Actor>)> {
        if let Err(e) = self.validate_player_corpse(source, &corpse) {
            return Err((e, Box::new(corpse)));
        }
        if state.source != source || state.operation == 0 || state.items.len() > 4096 {
            return Err((WorldError::InvalidVital, Box::new(corpse)));
        }
        let id = corpse.id;
        self.insert_anchor(corpse)?;
        self.corpses.insert(id, state);
        Ok(())
    }
    pub fn reset_player_death_history(&mut self, actor: EntityId) -> Result<(), WorldError> {
        let ready = if self.has_death_motions(actor) {
            let profile = self
                .locomotion_style(actor, 0x8000003d)
                .cloned()
                .ok_or(WorldError::InvalidMotion)?;
            self.body(actor)?
                .validate_animated_style(&profile, true)
                .map_err(|_| WorldError::InvalidMotion)?;
            Some(profile)
        } else {
            None
        };
        let c = self
            .combatants
            .get_mut(&actor)
            .ok_or(WorldError::MissingActor)?;
        if !c.profile().player {
            return Err(WorldError::LivingActor);
        }
        c.reset_damage_history();
        c.set_mode(1);
        if let Some(profile) = ready {
            // This explicit respawn owner may still be in server portal space;
            // ordinary body_mut intentionally refuses that state.
            self.actors
                .get_mut(&actor)
                .expect("respawn body preflight")
                .body
                .adopt_animated_style(profile, true)
                .expect("respawn Ready preflight");
        }
        Ok(())
    }
}
impl World {
    pub fn validate_player_respawn(
        &self,
        actor: EntityId,
        changes: &[bace_entity::VitalMutation],
        maxima: [u32; 3],
        token: Option<VitalReservationToken>,
    ) -> Result<(), WorldError> {
        if changes.len() != 3 || maxima.iter().any(|v| *v == 0 || *v > i32::MAX as u32) {
            return Err(WorldError::InvalidVital);
        }
        self.validate_health_observations([actor])?;
        self.validate_vital_reservations(changes, token)?;
        let c = self
            .combatants
            .get(&actor)
            .ok_or(WorldError::MissingActor)?;
        if !c.profile().player || c.health() != 0 || c.revision() == u64::MAX {
            return Err(WorldError::InvalidVital);
        }
        for (vital, maximum) in [
            bace_entity::EntityVital::Health,
            bace_entity::EntityVital::Stamina,
            bace_entity::EntityVital::Mana,
        ]
        .into_iter()
        .zip(maxima)
        {
            let p = c.vital(vital).ok_or(WorldError::InvalidVital)?;
            if changes
                .iter()
                .filter(|change| {
                    change.actor == actor
                        && change.vital == vital
                        && change.before == p.current
                        && change.after == (maximum as f32 * 0.75).round_ties_even() as u32
                })
                .count()
                != 1
            {
                return Err(WorldError::InvalidVital);
            }
        }
        Ok(())
    }
    pub fn apply_player_respawn(
        &mut self,
        actor: EntityId,
        changes: &[bace_entity::VitalMutation],
        maxima: [u32; 3],
        token: VitalReservationToken,
    ) -> Result<(), WorldError> {
        self.validate_player_respawn(actor, changes, maxima, Some(token))?;
        self.combatants
            .get_mut(&actor)
            .expect("validated combatant")
            .restore_after_death(maxima)
            .map_err(|_| WorldError::InvalidVital)?;
        self.publish_health_observation(actor);
        Ok(())
    }
}
