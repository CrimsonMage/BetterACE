use super::{World, WorldError};
use bace_geometry::Vec3;
use bace_physics::{ProjectileBody, ProjectileHit, ProjectileStep};
use bace_types::{CellId, EntityId};

pub struct OwnedProjectile {
    pub cell: CellId,
    pub source: EntityId,
    pub target: Option<EntityId>,
    pub body: ProjectileBody,
}
impl World {
    pub fn actor_in_frame(
        &self,
        actor: EntityId,
        frame: CellId,
    ) -> Result<(Vec3, Vec3), WorldError> {
        let (cell, state) = self.actor_state(actor)?;
        let offset = if cell == frame {
            Vec3::ZERO
        } else {
            self.geometry
                .as_ref()
                .ok_or(WorldError::MissingGeometry)?
                .frame_offset(frame.0, cell.0)
                .map_err(bace_physics::PhysicsError::from)?
        };
        let position = offset + state.position();
        if !position.is_finite() {
            return Err(bace_physics::PhysicsError::InvalidState.into());
        }
        Ok((position, state.velocity()))
    }
    pub fn projectile(&self, id: EntityId) -> Option<&OwnedProjectile> {
        self.projectiles.get(&id)
    }
    pub fn insert_projectile(
        &mut self,
        id: EntityId,
        projectile: OwnedProjectile,
    ) -> Result<(), (WorldError, OwnedProjectile)> {
        if id.0 == 0 || self.contains_identity(id) || self.projectiles.len() >= 4096 {
            return Err((WorldError::DuplicateActor, projectile));
        }
        let valid = if let Some(region) = self
            .geometry
            .as_ref()
            .filter(|r| r.cell(projectile.cell.0).is_some())
        {
            bace_physics::CollisionShape::prepare(
                vec![bace_physics::CollisionSphere {
                    center: Vec3::ZERO,
                    radius: projectile.body.radius(),
                }],
                0.0,
                0.0,
            )
            .is_ok_and(|shape| {
                region
                    .validate_placement(
                        projectile.cell.0,
                        projectile.body.position(),
                        &shape,
                        &[],
                        0,
                    )
                    .is_ok()
            })
        } else {
            self.scene(projectile.cell).is_ok_and(|scene| {
                scene.valid_placement(projectile.body.position(), projectile.body.radius())
            })
        };
        if !valid {
            return Err((WorldError::MissingGeometry, projectile));
        }
        self.projectile_frames
            .insert(id, (projectile.cell, Vec3::ZERO));
        self.visibility.track(id, projectile.cell);
        self.projectiles.insert(id, projectile);
        Ok(())
    }
    pub fn remove_projectile(&mut self, id: EntityId) -> Option<OwnedProjectile> {
        let removed = self.projectiles.remove(&id);
        if removed.is_some() {
            self.visibility.untrack(id);
        }
        self.projectile_frames.remove(&id);
        if let Some(projectile) = &removed {
            self.retire_damage_owner(projectile.source);
        }
        removed
    }
    pub fn step_projectile(
        &mut self,
        id: EntityId,
        seconds: f32,
    ) -> Result<ProjectileStep, WorldError> {
        self.step_projectile_filtered(id, seconds, |_, _| true)
    }
    /// Actor eligibility is supplied by the authoritative gameplay owner. Static
    /// geometry is always swept; callbacks must be bounded, read-only and no-I/O.
    pub fn step_projectile_filtered(
        &mut self,
        id: EntityId,
        seconds: f32,
        mut accept: impl FnMut(&World, EntityId) -> bool,
    ) -> Result<ProjectileStep, WorldError> {
        let projectile = self.projectiles.get(&id).ok_or(WorldError::MissingActor)?;
        if projectile.body.finished() {
            return Ok(ProjectileStep::Finished);
        }
        let from = projectile.body.position();
        let to = projectile.body.endpoint(seconds)?;
        let hit = self.sweep_projectile_filtered(
            projectile.cell,
            from,
            to,
            projectile.body.radius(),
            projectile.source,
            &mut accept,
        )?;
        let mut next = projectile.body.clone();
        let result = next.step(seconds, hit)?;
        let transition = {
            self.geometry
                .as_ref()
                .filter(|r| r.cell(projectile.cell.0).is_some())
                .map(|r| {
                    r.trace_sphere(
                        projectile.cell.0,
                        from,
                        next.position(),
                        projectile.body.radius(),
                    )
                })
                .transpose()
                .map_err(bace_physics::PhysicsError::from)?
        };
        let (origin, mut offset) = *self
            .projectile_frames
            .get(&id)
            .ok_or(WorldError::MissingGeometry)?;
        let mut cell = projectile.cell;
        if let Some(transition) = transition {
            next.translate_frame(transition.translation)?;
            offset = offset + transition.translation;
            if !offset.is_finite() {
                return Err(bace_physics::PhysicsError::InvalidState.into());
            }
            cell = CellId(transition.destination);
        }
        let projectile = self
            .projectiles
            .get_mut(&id)
            .ok_or(WorldError::MissingActor)?;
        projectile.body = next;
        if projectile.cell != cell {
            self.visibility.invalidate();
        }
        projectile.cell = cell;
        self.projectile_frames.insert(id, (origin, offset));
        Ok(result)
    }
    /// Straight distance from the immutable initial-cast pose, expressed in the
    /// launch cell's coordinate frame. Accepted portal translations—not guessed
    /// world constants—keep this correct across admitted cell transitions.
    pub fn projectile_distance_from(
        &self,
        id: EntityId,
        cell: CellId,
        position: Vec3,
    ) -> Result<f32, WorldError> {
        let projectile = self.projectiles.get(&id).ok_or(WorldError::MissingActor)?;
        let (origin, offset) = self
            .projectile_frames
            .get(&id)
            .ok_or(WorldError::MissingGeometry)?;
        if *origin != cell || !position.is_finite() {
            return Err(WorldError::MissingGeometry);
        }
        let squared = (projectile.body.position() - *offset - position).length_squared();
        if !squared.is_finite() {
            return Err(bace_physics::PhysicsError::InvalidState.into());
        }
        Ok(squared.sqrt())
    }
    /// Current synthetic same-cell collision: first world obstacle or accepted
    /// actor sphere. Unsupported cell traversal returns an error, never clear.
    pub fn sweep_projectile(
        &self,
        cell: CellId,
        from: Vec3,
        to: Vec3,
        radius: f32,
        ignore_actor: EntityId,
    ) -> Result<Option<ProjectileHit>, WorldError> {
        self.sweep_projectile_filtered(cell, from, to, radius, ignore_actor, &mut |_, _| true)
    }
    #[allow(clippy::too_many_arguments)]
    fn sweep_projectile_filtered(
        &self,
        cell: CellId,
        from: Vec3,
        to: Vec3,
        radius: f32,
        ignore_actor: EntityId,
        accept: &mut impl FnMut(&World, EntityId) -> bool,
    ) -> Result<Option<ProjectileHit>, WorldError> {
        let trace = self
            .geometry
            .as_ref()
            .filter(|r| r.cell(cell.0).is_some())
            .map(|r| r.trace_sphere(cell.0, from, to, radius))
            .transpose()
            .map_err(bace_physics::PhysicsError::from)?;
        let mut nearest = if let Some(trace) = trace {
            trace.contact.map(|h| ProjectileHit {
                fraction: h.fraction,
                target: None,
            })
        } else {
            self.scene(cell)?
                .swept_fraction(from, to, radius)?
                .map(|fraction| ProjectileHit {
                    fraction,
                    target: None,
                })
        };
        let delta = to - from;
        for actor in self.actors.values().filter(|actor| {
            (actor.cell == cell || trace.is_some_and(|t| actor.cell.0 == t.destination))
                && actor.id != ignore_actor
                && !self.anchors.contains(&actor.id)
                && !self.is_in_portal_transit(actor.id)
        }) {
            if !accept(self, actor.id) {
                continue;
            }
            let frame = if actor.cell == cell {
                Vec3::ZERO
            } else {
                trace.map_or(Vec3::ZERO, |t| t.translation)
            };
            let mut check = |sphere: bace_physics::CollisionSphere,
                             cylinder_height: Option<f32>| {
                let obstacle = bace_physics::DynamicSphere {
                    player_status: self.actor_player_status(actor.id),
                    object: actor.id.0,
                    cell: actor.cell.0,
                    sphere: bace_physics::CollisionSphere {
                        center: actor.body.accepted().position() + sphere.center,
                        radius: sphere.radius,
                    },
                    cylinder_height,
                };
                let fraction = if obstacle.overlaps(bace_physics::CollisionSphere {
                    center: from + frame,
                    radius,
                }) {
                    Some(0.0)
                } else {
                    obstacle
                        .cast(from + frame, delta, radius)
                        .map(|h| h.fraction)
                };
                if let Some(fraction) = fraction
                    && nearest.is_none_or(|old| fraction < old.fraction)
                {
                    nearest = Some(ProjectileHit {
                        fraction,
                        target: Some(actor.id.0),
                    });
                }
            };
            if let Some(shape) = actor.body.collision_shape() {
                for (sphere, height) in shape.obstacles() {
                    check(sphere, height);
                }
            } else {
                check(
                    bace_physics::CollisionSphere {
                        center: Vec3::ZERO,
                        radius: actor.body.collision_radius(),
                    },
                    None,
                );
            }
        }
        Ok(nearest)
    }
}
