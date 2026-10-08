//! Atomic server-authored group teleports; every participant is preflighted before
//! the first body mutation. No waits or callbacks can interleave the commit.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldTeleport {
    pub actor: EntityId,
    pub expected_epoch: u16,
    pub destination: CellId,
    pub position: Vec3,
    pub heading: f32,
}
impl World {
    pub fn validate_teleport_batch(&self, requests: &[WorldTeleport]) -> Result<(), WorldError> {
        if requests.is_empty() || requests.len() > 9 {
            return Err(WorldError::InvalidTeleportBatch);
        }
        let mut ids = std::collections::BTreeSet::new();
        for request in requests {
            let actor = self
                .actors
                .get(&request.actor)
                .ok_or(WorldError::MissingActor)?;
            if !ids.insert(request.actor)
                || self.anchors.contains(&request.actor)
                || self.retirement_holds.contains_key(&request.actor)
                || actor.body.accepted().epoch() != request.expected_epoch
                || !request.position.is_finite()
                || !request.heading.is_finite()
            {
                return Err(WorldError::InvalidTeleportBatch);
            }
        }
        let mut dynamics = Vec::new();
        for actor in self
            .actors
            .values()
            .filter(|a| !self.anchors.contains(&a.id) && !self.is_in_portal_transit(a.id))
        {
            let (cell, position) = requests
                .iter()
                .find(|r| r.actor == actor.id)
                .map_or((actor.cell, actor.body.accepted().position()), |r| {
                    (r.destination, r.position)
                });
            let status = self.actor_player_status(actor.id);
            let mut add = |sphere: bace_physics::CollisionSphere, cylinder_height: Option<f32>| {
                dynamics.push(bace_physics::DynamicSphere {
                    object: actor.id.0,
                    cell: cell.0,
                    sphere: bace_physics::CollisionSphere {
                        center: position + sphere.center,
                        radius: sphere.radius,
                    },
                    cylinder_height,
                    player_status: status,
                })
            };
            if let Some(shape) = actor.body.collision_shape() {
                for (sphere, height) in shape.obstacles() {
                    add(sphere, height);
                }
            } else {
                add(
                    bace_physics::CollisionSphere {
                        center: Vec3::ZERO,
                        radius: actor.body.collision_radius(),
                    },
                    None,
                );
            }
            if dynamics.len() > 65536 {
                return Err(WorldError::InvalidTeleportBatch);
            }
        }
        for request in requests {
            let body = &self
                .actors
                .get(&request.actor)
                .ok_or(WorldError::MissingActor)?
                .body;
            if let Some(shape) = body.collision_shape() {
                let region = self.geometry.as_ref().ok_or(WorldError::MissingGeometry)?;
                if self.actor_player_status(request.actor).is_some() {
                    self.validate_player_cell_entry(request.actor, request.destination)?;
                }
                region
                    .validate_placement(
                        request.destination.0,
                        request.position,
                        shape,
                        &dynamics,
                        request.actor.0,
                    )
                    .map_err(PhysicsError::from)?;
            } else {
                if !self
                    .scene(request.destination)?
                    .valid_placement(request.position, body.collision_radius())
                {
                    return Err(PhysicsError::InvalidState.into());
                }
                let candidate = bace_physics::CollisionSphere {
                    center: request.position,
                    radius: body.collision_radius(),
                };
                if dynamics
                    .iter()
                    .filter(|d| {
                        d.cell == request.destination.0
                            && d.object != request.actor.0
                            && bace_physics::players_collide(
                                self.actor_player_status(request.actor),
                                d.player_status,
                            )
                    })
                    .any(|d| d.overlaps(candidate))
                {
                    return Err(PhysicsError::InvalidState.into());
                }
            }
        }
        Ok(())
    }
    pub fn teleport_batch(&mut self, requests: &[WorldTeleport]) -> Result<(), WorldError> {
        self.validate_teleport_batch(requests)?;
        for request in requests {
            let actor = self
                .actors
                .get_mut(&request.actor)
                .expect("preflight actor");
            if actor.body.collision_shape().is_some() {
                actor
                    .body
                    .teleport_geometry(
                        self.geometry.as_ref().expect("preflight region"),
                        request.destination.0,
                        request.position,
                        request.actor.0,
                        &[],
                    )
                    .expect("preflight static placement");
            } else {
                actor
                    .body
                    .server_teleport(
                        self.scenes
                            .get(&request.destination)
                            .expect("preflight scene"),
                        request.position,
                    )
                    .expect("preflight static placement");
            }
            actor
                .body
                .server_heading(request.heading)
                .expect("preflight heading");
            actor.cell = request.destination;
        }
        self.visibility.invalidate();
        Ok(())
    }
}
