mod health_observations;
pub use health_observations::HealthObservation;
mod anchors;
mod cell_access;
mod death_motions;
mod locomotion_styles;
mod motions;
mod npc_admission;
mod object_views;
mod player_death;
mod player_entry;
pub use npc_admission::WorldNpcAdmissionHold;
mod portal_transit;
mod pve_death;
mod residency;
mod retirement;
mod server_move;
mod shutdown;
mod teleports;
mod visibility;
pub use retirement::WorldRetirementHold;
mod vital_reservations;
pub use motions::WorldMotionEvent;
pub use teleports::WorldTeleport;
pub use visibility::{PreparedCellVisibility, VisibilityError, visibility_distance_squared};
pub use vital_reservations::{VitalReservationDomain, VitalReservationToken};
mod damage_owners;
mod geometry;
mod projectiles;
use bace_entity::{Actor, Combatant};
use bace_geometry::{Aabb, Vec3};
use bace_physics::{AcceptedState, Body, PhysicsError, SyntheticScene};
use bace_types::{CellId, EntityId};
pub use projectiles::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpseState {
    pub operation: u64,
    pub source: EntityId,
    pub template: u32,
    pub owner: Option<EntityId>,
    pub items: Vec<EntityId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoorCollider {
    pub cell: CellId,
    pub position: Vec3,
    pub bounds: Aabb,
    pub solid: bool,
}

/// Single-owner synthetic world harness. This is not an AC landblock loader.
#[derive(Default)]
pub struct World {
    health_observations: health_observations::HealthObservations,
    death_motion_cursor: Option<EntityId>,
    death_motions: BTreeMap<EntityId, std::sync::Arc<[bace_motion::PreparedDeathMotion]>>,
    locomotion_style_actors: BTreeSet<EntityId>,
    locomotion_styles: BTreeMap<(EntityId, u32), std::sync::Arc<bace_motion::AnimatedLocomotion>>,
    movement_goals: BTreeMap<
        EntityId,
        (
            bace_motion::TurnControl,
            bace_gameplay_api::visibility::ServerMovementGoal,
        ),
    >,
    retirement_holds: BTreeMap<EntityId, WorldRetirementHold>,
    visibility: visibility::VisibilityState,
    dormant_landblocks: std::collections::BTreeSet<u16>,
    vital_reservations: BTreeMap<(EntityId, bace_entity::EntityVital), VitalReservationToken>,
    motions: BTreeMap<EntityId, motions::OwnedMotion>,
    motion_last: BTreeMap<(EntityId, bace_motion::MotionDomain), (u64, u64)>,
    motion_events: std::collections::VecDeque<WorldMotionEvent>,
    motion_scratch: Vec<bace_motion::MotionPlaybackEvent>,
    motion_retired: Vec<EntityId>,
    motion_blocked: std::collections::BTreeSet<EntityId>,
    motion_phase: u64,
    motion_callback_budget: usize,
    motion_callbacks: BTreeMap<EntityId, motions::CompletedCallback>,
    portal_transit: BTreeMap<EntityId, u64>,
    entry_pending: std::collections::BTreeSet<EntityId>,
    npc_admissions: BTreeMap<EntityId, WorldNpcAdmissionHold>,
    next_npc_admission: u64,
    anchors: std::collections::BTreeSet<EntityId>,
    restriction_generations: BTreeMap<u32, u64>,
    cell_access: BTreeMap<EntityId, BTreeMap<u32, u64>>,
    damage_owners: BTreeMap<EntityId, EntityId>,
    scenes: BTreeMap<CellId, SyntheticScene>,
    geometry: Option<std::sync::Arc<bace_physics::GeometryRegion>>,
    geometry_dynamic: Vec<bace_physics::DynamicSphere>,
    geometry_blocked: BTreeMap<EntityId, bace_physics::GeometryError>,
    actors: BTreeMap<EntityId, Actor>,
    combatants: BTreeMap<EntityId, Combatant>,
    corpses: BTreeMap<EntityId, CorpseState>,
    doors: BTreeMap<EntityId, DoorCollider>,
    properties: BTreeMap<EntityId, bace_entity::EntityProperties>,
    projectiles: BTreeMap<EntityId, OwnedProjectile>,
    projectile_frames: BTreeMap<EntityId, (CellId, Vec3)>,
}

impl World {
    pub fn actor_state(&self, id: EntityId) -> Result<(CellId, AcceptedState), WorldError> {
        let actor = self.actors.get(&id).ok_or(WorldError::MissingActor)?;
        Ok((actor.cell, actor.body.accepted()))
    }
    pub fn vital(
        &self,
        id: EntityId,
        kind: bace_entity::EntityVital,
    ) -> Result<bace_entity::VitalPool, WorldError> {
        self.combatants
            .get(&id)
            .and_then(|c| c.vital(kind))
            .ok_or(WorldError::InvalidVital)
    }
    pub fn validate_vital_batch(
        &self,
        changes: &[bace_entity::VitalMutation],
        damage_source: Option<EntityId>,
    ) -> Result<(), WorldError> {
        self.validate_vital_reservations(changes, None)?;
        self.validate_vital_batch_values(changes, damage_source)
    }
    fn validate_vital_batch_values(
        &self,
        changes: &[bace_entity::VitalMutation],
        damage_source: Option<EntityId>,
    ) -> Result<(), WorldError> {
        if changes.len() > 64 || damage_source == Some(EntityId(0)) {
            return Err(WorldError::InvalidVital);
        }
        self.validate_health_observations(
            changes
                .iter()
                .filter(|c| c.vital == bace_entity::EntityVital::Health && c.before != c.after)
                .map(|c| c.actor),
        )?;
        for (index, change) in changes.iter().enumerate() {
            if changes[..index]
                .iter()
                .any(|old| old.actor == change.actor && old.vital == change.vital)
            {
                return Err(WorldError::InvalidVital);
            }
            let combatant = self
                .combatants
                .get(&change.actor)
                .ok_or(WorldError::InvalidVital)?;
            let pool = combatant
                .vital(change.vital)
                .ok_or(WorldError::InvalidVital)?;
            if pool.current != change.before || change.after > pool.maximum {
                return Err(WorldError::InvalidVital);
            }
            let increments = changes
                .iter()
                .filter(|c| c.actor == change.actor && c.before != c.after)
                .count() as u64;
            if combatant.revision().checked_add(increments).is_none() {
                return Err(WorldError::InvalidVital);
            }
            if change.vital == bace_entity::EntityVital::Health
                && change.after < change.before
                && damage_source.is_some_and(|source| !combatant.can_record_damage(source))
            {
                return Err(WorldError::InvalidVital);
            }
        }
        Ok(())
    }
    pub fn apply_vital_batch(
        &mut self,
        changes: &[bace_entity::VitalMutation],
        damage_source: Option<EntityId>,
    ) -> Result<Vec<bace_entity::VitalMutationResult>, WorldError> {
        self.validate_vital_batch(changes, damage_source)?;
        Ok(self.apply_validated_vital_batch(changes, damage_source))
    }
    fn apply_validated_vital_batch(
        &mut self,
        changes: &[bace_entity::VitalMutation],
        damage_source: Option<EntityId>,
    ) -> Vec<bace_entity::VitalMutationResult> {
        let mut result = Vec::with_capacity(changes.len());
        for change in changes {
            let combatant = self
                .combatants
                .get_mut(&change.actor)
                .expect("prevalidated combatant");
            combatant
                .apply_vital(change.vital, change.before, change.after, damage_source)
                .expect("prevalidated atomic vital batch");
            result.push(bace_entity::VitalMutationResult {
                mutation: *change,
                revision: combatant.revision(),
            });
            if change.vital == bace_entity::EntityVital::Health && change.before != change.after {
                self.publish_health_observation(change.actor);
            }
        }
        result
    }

    pub fn register_properties(
        &mut self,
        id: EntityId,
        properties: bace_entity::EntityProperties,
    ) -> Result<(), WorldError> {
        if !self.actors.contains_key(&id) && !self.doors.contains_key(&id) {
            return Err(WorldError::MissingActor);
        }
        if self.properties.contains_key(&id) {
            return Err(WorldError::DuplicateActor);
        }
        self.properties.insert(id, properties);
        Ok(())
    }
    pub fn properties(&self, id: EntityId) -> Option<&bace_entity::EntityProperties> {
        self.properties.get(&id)
    }
    pub fn properties_mut(&mut self, id: EntityId) -> Option<&mut bace_entity::EntityProperties> {
        if self.retirement_holds.contains_key(&id) {
            return None;
        }
        self.properties.get_mut(&id)
    }

    pub fn register_door(
        &mut self,
        id: EntityId,
        collider: DoorCollider,
    ) -> Result<(), WorldError> {
        if id.0 == 0 || self.contains_identity(id) || self.doors.len() >= 4096 {
            return Err(WorldError::DuplicateActor);
        }
        if !collider.position.is_finite() {
            return Err(WorldError::Physics(PhysicsError::InvalidState));
        }
        if collider.solid
            && self.actors.values().any(|actor| {
                actor.cell == collider.cell
                    && collider.bounds.intersects_sphere(
                        actor.body.accepted().position(),
                        actor.body.collision_radius(),
                    )
            })
        {
            return Err(WorldError::Physics(PhysicsError::InvalidState));
        }
        if let Some(region) = self
            .geometry
            .as_mut()
            .filter(|r| r.cell(collider.cell.0).is_some())
        {
            std::sync::Arc::make_mut(region)
                .set_object_solid(id.0, collider.solid)
                .map_err(PhysicsError::from)?;
        } else {
            let scene = self
                .scenes
                .get_mut(&collider.cell)
                .ok_or(WorldError::MissingGeometry)?;
            scene.register_dynamic_obstacle(id.0, collider.bounds, collider.solid)?;
        }
        self.doors.insert(id, collider);
        Ok(())
    }
    pub fn door(&self, id: EntityId) -> Option<DoorCollider> {
        self.doors.get(&id).copied()
    }
    pub fn door_occupied(&self, id: EntityId) -> Result<bool, WorldError> {
        let door = self.doors.get(&id).ok_or(WorldError::MissingActor)?;
        if self
            .geometry
            .as_ref()
            .is_none_or(|r| r.cell(door.cell.0).is_none())
        {
            self.scene(door.cell)?;
        }
        Ok(self.actors.values().any(|actor| {
            actor.id != id
                && actor.cell == door.cell
                && door.bounds.intersects_sphere(
                    actor.body.accepted().position(),
                    actor.body.collision_radius(),
                )
        }))
    }
    pub fn set_door_solid(&mut self, id: EntityId, solid: bool) -> Result<(), WorldError> {
        let door = self.doors.get_mut(&id).ok_or(WorldError::MissingActor)?;
        if let Some(region) = self
            .geometry
            .as_mut()
            .filter(|r| r.cell(door.cell.0).is_some())
        {
            std::sync::Arc::make_mut(region)
                .set_object_solid(id.0, solid)
                .map_err(PhysicsError::from)?;
        } else {
            self.scenes
                .get_mut(&door.cell)
                .ok_or(WorldError::MissingGeometry)?
                .set_dynamic_solid(id.0, solid)?;
        }
        door.solid = solid;
        Ok(())
    }
    pub fn door_use_geometry(
        &self,
        actor: EntityId,
        id: EntityId,
        range: f32,
    ) -> Result<(bool, bool), WorldError> {
        let actor = self.actors.get(&actor).ok_or(WorldError::MissingActor)?;
        let door = self.doors.get(&id).ok_or(WorldError::MissingActor)?;
        if actor.cell != door.cell {
            return Ok((false, false));
        }
        let position = actor.body.accepted().position();
        Ok((
            (door.position - position).length_squared() <= range * range,
            self.segment_clear(door.cell, position, door.position, Some(id.0))?,
        ))
    }

    pub fn corpse(&self, actor: EntityId) -> Option<&CorpseState> {
        self.corpses.get(&actor)
    }
    pub fn contains_identity(&self, id: EntityId) -> bool {
        self.health_observation_mentions(id)
            || self.actors.contains_key(&id)
            || self.motions.contains_key(&id)
            || self.motion_events.iter().any(|event| event.actor == id)
            || self.damage_owners.contains_key(&id)
            || self.damage_owners.values().any(|owner| *owner == id)
            || self.projectiles.contains_key(&id)
            || self.doors.contains_key(&id)
            || self.corpses.values().any(|c| c.items.contains(&id))
    }
    /// Apply only a confirmed durable transfer; the destination inventory is
    /// adopted by its owning aggregate in the same completion phase.
    pub fn confirm_corpse_item_removed(&mut self, corpse: EntityId, item: EntityId) -> bool {
        let Some(state) = self.corpses.get_mut(&corpse) else {
            return false;
        };
        let Some(index) = state.items.iter().position(|id| *id == item) else {
            return false;
        };
        state.items.remove(index);
        true
    }
    pub fn create_corpse(
        &mut self,
        source: EntityId,
        corpse: EntityId,
        state: CorpseState,
    ) -> Result<(), WorldError> {
        if self.health_observation_pending_for(source) {
            return Err(WorldError::HealthBackpressure);
        }
        if self.has_reserved_vitals(source) {
            return Err(WorldError::VitalReserved);
        }
        if corpse.0 == 0 || self.contains_identity(corpse) {
            return Err(WorldError::DuplicateActor);
        }
        if self.combatants.get(&source).is_none_or(|s| s.health() != 0) {
            return Err(WorldError::LivingActor);
        }
        let mut actor = self
            .actors
            .remove(&source)
            .ok_or(WorldError::MissingActor)?;
        self.retire_health_subscriptions(source);
        self.combatants.remove(&source);
        self.properties.remove(&source);
        self.retire_damage_owner(source);
        actor.id = corpse;
        self.actors.insert(corpse, actor);
        self.movement_goals.remove(&source);
        self.visibility.invalidate();
        self.corpses.insert(corpse, state);
        Ok(())
    }
    pub fn remove(&mut self, actor: EntityId) -> Option<Actor> {
        if self.health_observation_pending_for(actor)
            || self.has_reserved_vitals(actor)
            || self.retirement_holds.contains_key(&actor)
        {
            return None;
        }
        self.retire_health_subscriptions(actor);
        self.retire_removed_actor_motion(actor);
        self.locomotion_styles.retain(|(id, _), _| *id != actor);
        self.locomotion_style_actors.remove(&actor);
        self.death_motions.remove(&actor);
        self.portal_transit.remove(&actor);
        self.entry_pending.remove(&actor);
        self.npc_admissions.remove(&actor);
        self.anchors.remove(&actor);
        self.cell_access.remove(&actor);
        self.geometry_blocked.remove(&actor);
        self.properties.remove(&actor);
        self.combatants.remove(&actor);
        self.corpses.remove(&actor);
        let removed = self.actors.remove(&actor);
        self.movement_goals.remove(&actor);
        self.visibility.invalidate();
        self.retire_damage_owner(actor);
        removed
    }
    pub fn register_combatant(
        &mut self,
        actor: EntityId,
        combatant: Combatant,
    ) -> Result<(), WorldError> {
        if !self.actors.contains_key(&actor) {
            return Err(WorldError::MissingActor);
        }
        if self.combatants.contains_key(&actor) {
            return Err(WorldError::DuplicateActor);
        }
        self.combatants.insert(actor, combatant);
        Ok(())
    }
    pub fn combatant(&self, actor: EntityId) -> Option<&Combatant> {
        self.combatants.get(&actor)
    }
    pub fn combatant_mut(&mut self, actor: EntityId) -> Option<&mut Combatant> {
        self.combatants.get_mut(&actor)
    }
    pub fn attack_geometry(
        &self,
        actor: EntityId,
        target: EntityId,
        range: f32,
    ) -> Result<(bool, bool), WorldError> {
        if self.is_in_portal_transit(actor) || self.is_in_portal_transit(target) {
            return Ok((false, false));
        }
        let actor = self.actors.get(&actor).ok_or(WorldError::MissingActor)?;
        let target = self.actors.get(&target).ok_or(WorldError::MissingActor)?;
        if actor.cell != target.cell {
            return Ok((false, false));
        }
        let from = actor.body.accepted().position();
        let to = target.body.accepted().position();
        let in_range = (to - from).length_squared() <= range * range;
        Ok((in_range, self.segment_clear(actor.cell, from, to, None)?))
    }
    pub fn register_scene(
        &mut self,
        cell: CellId,
        scene: SyntheticScene,
    ) -> Result<(), WorldError> {
        if self.scenes.contains_key(&cell) {
            return Err(WorldError::DuplicateScene);
        }
        self.scenes.insert(cell, scene);
        Ok(())
    }
    pub fn scene(&self, cell: CellId) -> Result<&SyntheticScene, WorldError> {
        self.scenes.get(&cell).ok_or(WorldError::MissingGeometry)
    }
    pub fn insert(&mut self, actor: Actor) -> Result<(), WorldError> {
        self.validate_actor(&actor)?;
        self.visibility.track(actor.id, actor.cell);
        self.actors.insert(actor.id, actor);
        Ok(())
    }
    /// Trusted loading-player transfer; normal insertion keeps dynamic overlap
    /// rejection. Static geometry and identity still gate this insertion.
    pub fn insert_loading_player(&mut self, actor: Actor) -> Result<(), WorldError> {
        self.validate_loading_player_actor(&actor)?;
        self.visibility.track(actor.id, actor.cell);
        self.actors.insert(actor.id, actor);
        Ok(())
    }
    pub fn body(&self, id: EntityId) -> Result<&Body, WorldError> {
        self.actors
            .get(&id)
            .map(|a| &a.body)
            .ok_or(WorldError::MissingActor)
    }
    /// Called by the simulation owner with validated commands, not session code.
    pub fn body_mut(&mut self, id: EntityId) -> Result<&mut Body, WorldError> {
        if self.retirement_holds.contains_key(&id) {
            return Err(WorldError::VitalReserved);
        }
        if self.anchors.contains(&id) || self.portal_transit.contains_key(&id) {
            return Err(WorldError::InvalidTeleportBatch);
        }
        if self.combatants.get(&id).is_some_and(|s| s.health() == 0)
            || self.corpses.contains_key(&id)
        {
            return Err(WorldError::DeadActor);
        }
        self.actors
            .get_mut(&id)
            .map(|a| &mut a.body)
            .ok_or(WorldError::MissingActor)
    }
    pub fn states(&self) -> impl Iterator<Item = (EntityId, CellId, AcceptedState)> + '_ {
        self.actors
            .values()
            .map(|a| (a.id, a.cell, a.body.accepted()))
    }
    pub fn tick(&mut self) -> Result<(), WorldError> {
        self.start_pending_death_motions();
        self.advance_motions()?;
        self.refresh_geometry_dynamic()?;
        for actor in self.actors.values_mut() {
            if (self
                .dormant_landblocks
                .contains(&((actor.cell.0 >> 16) as u16))
                && self
                    .motions
                    .get(&actor.id)
                    .is_none_or(|m| m.playback.token.domain != bace_motion::MotionDomain::Death))
                || self.anchors.contains(&actor.id)
                || self.portal_transit.contains_key(&actor.id)
                || self.retirement_holds.contains_key(&actor.id)
            {
                continue;
            }
            if (self
                .combatants
                .get(&actor.id)
                .is_some_and(|s| s.health() == 0)
                || self.corpses.contains_key(&actor.id))
                && !self.motions.get(&actor.id).is_some_and(|m| {
                    m.epoch == actor.body.accepted().epoch()
                        && m.playback.token.domain == bace_motion::MotionDomain::Death
                })
            {
                actor.body.stop_motion();
            }
            if actor.body.collision_shape().is_some() {
                let region = self.geometry.as_ref().ok_or(WorldError::MissingGeometry)?;
                // Pinned ACE Player.InitPhysicsObj keeps IgnoreCollisions on
                // through the entry pink bubble. Static DAT geometry still
                // participates; only dynamic actor contacts are suppressed.
                let pending_entry = self.entry_pending.contains(&actor.id);
                let dynamics = if pending_entry {
                    &[][..]
                } else {
                    self.geometry_dynamic.as_slice()
                };
                let mut allowed = [0u32; 64];
                let mut count = 0;
                if let Some(grants) = self.cell_access.get(&actor.id) {
                    for (restriction, generation) in grants {
                        if self.restriction_generations.get(restriction) == Some(generation) {
                            allowed[count] = *restriction;
                            count += 1;
                        }
                    }
                }
                match actor.body.step_geometry_with_access(
                    region,
                    actor.cell.0,
                    actor.id.0,
                    dynamics,
                    &allowed[..count],
                    // The loading actor is known to be a player. Suppressing
                    // dynamic contacts must not suppress player-only authored
                    // cell-access checks at a portal boundary.
                    pending_entry.then_some(2),
                ) {
                    Ok(cell) => {
                        if actor.cell != CellId(cell) {
                            self.visibility.invalidate();
                        }
                        actor.cell = CellId(cell);
                    }
                    Err(PhysicsError::Geometry(error)) => {
                        actor.body.stop_motion();
                        self.geometry_blocked.insert(actor.id, error);
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                }
                let position = actor.body.accepted().position();
                if let Some(shape) = actor.body.collision_shape() {
                    for (entry, (sphere, _)) in self
                        .geometry_dynamic
                        .iter_mut()
                        .filter(|v| v.object == actor.id.0)
                        .zip(shape.obstacles())
                    {
                        entry.cell = actor.cell.0;
                        entry.sphere.center = position + sphere.center;
                    }
                }
            } else {
                let scene = self
                    .scenes
                    .get(&actor.cell)
                    .ok_or(WorldError::MissingGeometry)?;
                actor.body.step(scene);
            }
        }
        Ok(())
    }
    pub fn teleport(
        &mut self,
        id: EntityId,
        destination: CellId,
        position: Vec3,
    ) -> Result<(), WorldError> {
        if self.anchors.contains(&id) || self.retirement_holds.contains_key(&id) {
            return Err(WorldError::InvalidTeleportBatch);
        }

        if self
            .actors
            .get(&id)
            .is_some_and(|a| a.body.collision_shape().is_some())
        {
            if self.actor_player_status(id).is_some() {
                self.validate_player_cell_entry(id, destination)?;
            }
            self.refresh_geometry_dynamic()?;
            let region = self.geometry.as_ref().ok_or(WorldError::MissingGeometry)?;
            let actor = self.actors.get_mut(&id).ok_or(WorldError::MissingActor)?;
            actor.body.teleport_geometry(
                region,
                destination.0,
                position,
                id.0,
                &self.geometry_dynamic,
            )?;
            actor.cell = destination;
            self.visibility.invalidate();
            return Ok(());
        }
        let scene = self
            .scenes
            .get(&destination)
            .ok_or(WorldError::MissingGeometry)?;
        let actor = self.actors.get_mut(&id).ok_or(WorldError::MissingActor)?;
        actor.body.server_teleport(scene, position)?;
        actor.cell = destination;
        self.visibility.invalidate();
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("accepted health notification output is full")]
    HealthBackpressure,
    #[error("authoritative vital is reserved by a pending durable operation")]
    VitalReserved,
    #[error("invalid or stale server motion program/token")]
    InvalidMotion,
    #[error("actor already has an active server motion")]
    MotionBusy,
    #[error("server motion event output is full")]
    MotionBackpressure,
    #[error("invalid server teleport batch or immutable anchor mutation")]
    InvalidTeleportBatch,
    #[error("invalid, stale or exhausted cell-access projection")]
    InvalidCellAccess,
    #[error("invalid, conflicting or exhausted damage ownership relation")]
    InvalidDamageOwner,
    #[error("invalid, absent, conflicted or exhausted authoritative vital")]
    InvalidVital,
    #[error("living or missing combatant cannot become a corpse")]
    LivingActor,
    #[error("dead actors and corpses cannot submit movement intent")]
    DeadActor,
    #[error("cell has no validated collision geometry")]
    MissingGeometry,
    #[error("scene already registered")]
    DuplicateScene,
    #[error("actor ID already owned")]
    DuplicateActor,
    #[error("actor overlaps accepted actor {0:?}")]
    ActorOverlap(EntityId),
    #[error("actor does not exist")]
    MissingActor,
    #[error(transparent)]
    Physics(#[from] PhysicsError),
}

mod equipment;
