//! Geometry generation adoption shares the world's existing body owner.
use super::*;
use bace_physics::{CollisionSphere, DynamicSphere, GeometryRegion};
impl World {
    pub(super) fn actor_player_status(&self, id: EntityId) -> Option<u32> {
        self.combatants
            .get(&id)
            .filter(|c| c.profile().player)
            .map(|_| {
                match self
                    .properties
                    .get(&id)
                    .and_then(|p| p.get(bace_entity::PropertyFamily::Int, 134))
                {
                    Some(bace_entity::PropertyValue::Int(status)) => *status as u32,
                    _ => 2,
                }
            })
    }

    /// Geometry failure is observable and actor-local; never stop the simulation
    /// thread or accept an unchecked position because a neighbor is unavailable.
    pub fn take_geometry_blocked(&mut self) -> Option<(EntityId, bace_physics::GeometryError)> {
        self.geometry_blocked.pop_first()
    }

    pub fn validate_actor(&self, actor: &Actor) -> Result<(), WorldError> {
        self.validate_actor_in_region(actor, self.geometry.as_deref(), &[])
    }
    fn validate_actor_in_region(
        &self,
        actor: &Actor,
        region: Option<&GeometryRegion>,
        staged: &[Actor],
    ) -> Result<(), WorldError> {
        if self.contains_identity(actor.id) || staged.iter().any(|a| a.id == actor.id) {
            return Err(WorldError::DuplicateActor);
        }
        if let Some(shape) = actor.body.collision_shape() {
            let region = region.ok_or(WorldError::MissingGeometry)?;
            region
                .validate_placement(
                    actor.cell.0,
                    actor.body.accepted().position(),
                    shape,
                    &[],
                    actor.id.0,
                )
                .map_err(PhysicsError::from)?;
            for other in self.actors.values().chain(staged).filter(|o| {
                o.cell == actor.cell
                    && !self.anchors.contains(&o.id)
                    && !self.is_in_portal_transit(o.id)
            }) {
                let Some(other_shape) = other.body.collision_shape() else {
                    continue;
                };
                for sphere in shape.spheres() {
                    for (other_sphere, cylinder_height) in other_shape.obstacles() {
                        let obstacle = DynamicSphere {
                            player_status: self.actor_player_status(other.id),
                            object: other.id.0,
                            cell: other.cell.0,
                            sphere: CollisionSphere {
                                center: other.body.accepted().position() + other_sphere.center,
                                radius: other_sphere.radius,
                            },
                            cylinder_height,
                        };
                        if obstacle.overlaps(CollisionSphere {
                            center: actor.body.accepted().position() + sphere.center,
                            radius: sphere.radius,
                        }) {
                            return Err(PhysicsError::InvalidState.into());
                        }
                    }
                }
            }
        } else {
            actor.body.validate_placement(self.scene(actor.cell)?)?;
        }
        Ok(())
    }
    /// Atomic cold-region admission. Every candidate validates against the same
    /// immutable region and existing/earlier candidate bodies before ownership.
    pub fn install_geometry_and_actors(
        &mut self,
        region: std::sync::Arc<GeometryRegion>,
        actors: Vec<Actor>,
    ) -> Result<(), WorldError> {
        self.install_geometry_with_world_items(region, actors, Vec::new())
    }
    /// Restored world roots are ethereal anchors in the same actor registry.
    /// All geometry, identities and corpse metadata validate before adoption.
    pub fn install_geometry_with_world_items(
        &mut self,
        mut region: std::sync::Arc<GeometryRegion>,
        actors: Vec<Actor>,
        roots: Vec<(Actor, Option<CorpseState>)>,
    ) -> Result<(), WorldError> {
        if actors.len() + roots.len() > 4096 || self.anchors.len() + roots.len() > 4096 {
            return Err(PhysicsError::InvalidState.into());
        }
        for (id, door) in &self.doors {
            if region.cell(door.cell.0).is_some() {
                std::sync::Arc::make_mut(&mut region)
                    .set_object_solid(id.0, door.solid)
                    .map_err(PhysicsError::from)?;
            }
        }
        for (index, actor) in actors.iter().enumerate() {
            if actor.body.collision_shape().is_none() {
                return Err(WorldError::MissingGeometry);
            }
            self.validate_actor_in_region(actor, Some(&region), &actors[..index])?;
        }
        let mut seen = std::collections::BTreeSet::new();
        for (actor, corpse) in &roots {
            if actor.id.0 == 0
                || actor.id.0 == u32::MAX
                || self.contains_identity(actor.id)
                || actors.iter().any(|a| a.id == actor.id)
                || !seen.insert(actor.id)
                || corpse
                    .as_ref()
                    .is_some_and(|c| c.operation == 0 || c.source.0 == 0 || c.items.len() > 4096)
            {
                return Err(WorldError::DuplicateActor);
            }
            let shape = actor
                .body
                .collision_shape()
                .ok_or(WorldError::MissingGeometry)?;
            region
                .validate_placement(
                    actor.cell.0,
                    actor.body.accepted().position(),
                    shape,
                    &[],
                    actor.id.0,
                )
                .map_err(PhysicsError::from)?;
        }
        self.install_geometry(region)?;
        for (actor, corpse) in roots {
            self.anchors.insert(actor.id);
            if let Some(corpse) = corpse {
                self.corpses.insert(actor.id, corpse);
            }
            self.actors.insert(actor.id, actor);
        }
        for actor in actors {
            self.actors.insert(actor.id, actor);
        }
        self.visibility.invalidate();
        Ok(())
    }
    pub fn prepare_geometry_body(
        &self,
        input: bace_physics::GeometrySpawn,
    ) -> Result<Body, WorldError> {
        let cell = CellId(input.cell);
        let body = Body::spawn_geometry(
            self.geometry.as_ref().ok_or(WorldError::MissingGeometry)?,
            input,
        )?;
        // Identity0 is never a registered actor; used only for geometric validation.
        let actor = Actor {
            id: EntityId(0),
            cell,
            body,
        };
        self.validate_actor(&actor)?;
        Ok(actor.body)
    }

    /// Preparation happens elsewhere. Adoption validates all current bodies
    /// before replacing the immutable region; a failure retains the old region.
    pub fn install_geometry(
        &mut self,
        mut region: std::sync::Arc<GeometryRegion>,
    ) -> Result<(), WorldError> {
        // Preserve the owner-authorized state of doors across immutable asset
        // generation replacement. Never reopen/close them from cached defaults.
        for (id, door) in &self.doors {
            if region.cell(door.cell.0).is_some() {
                std::sync::Arc::make_mut(&mut region)
                    .set_object_solid(id.0, door.solid)
                    .map_err(PhysicsError::from)?;
            }
        }
        for actor in self.actors.values() {
            if let Some(shape) = actor.body.collision_shape() {
                region
                    .validate_placement(
                        actor.cell.0,
                        actor.body.accepted().position(),
                        shape,
                        &[],
                        actor.id.0,
                    )
                    .map_err(PhysicsError::from)?;
            }
        }
        self.geometry = Some(region);
        Ok(())
    }
    pub fn segment_clear(
        &self,
        cell: CellId,
        from: Vec3,
        to: Vec3,
        ignore: Option<u32>,
    ) -> Result<bool, WorldError> {
        if let Some(region) = self.geometry.as_ref().filter(|r| r.cell(cell.0).is_some()) {
            region
                .segment_clear(cell.0, from, to, ignore)
                .map_err(PhysicsError::from)
                .map_err(WorldError::from)
        } else {
            Ok(self.scene(cell)?.segment_clear_ignoring(from, to, ignore))
        }
    }
    pub fn geometry(&self) -> Option<&std::sync::Arc<GeometryRegion>> {
        self.geometry.as_ref()
    }
    pub(super) fn refresh_geometry_dynamic(&mut self) -> Result<(), WorldError> {
        let count = self
            .actors
            .values()
            .filter(|a| !self.anchors.contains(&a.id) && !self.is_in_portal_transit(a.id))
            .filter_map(|a| a.body.collision_shape())
            .map(|s| s.obstacles().count())
            .sum::<usize>();
        if count > 65536 {
            return Err(PhysicsError::Geometry(bace_physics::GeometryError::Budget).into());
        }
        self.geometry_dynamic.clear();
        if self.geometry_dynamic.capacity() < count {
            self.geometry_dynamic.reserve(count);
        }
        for actor in self.actors.values() {
            if self.anchors.contains(&actor.id) || self.is_in_portal_transit(actor.id) {
                continue;
            }
            if let Some(shape) = actor.body.collision_shape() {
                for (sphere, cylinder_height) in shape.obstacles() {
                    self.geometry_dynamic.push(DynamicSphere {
                        player_status: self.actor_player_status(actor.id),
                        cylinder_height,
                        object: actor.id.0,
                        cell: actor.cell.0,
                        sphere: CollisionSphere {
                            center: actor.body.accepted().position() + sphere.center,
                            radius: sphere.radius,
                        },
                    });
                }
            }
        }
        Ok(())
    }
}
