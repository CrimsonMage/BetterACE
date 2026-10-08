//! Static ethereal anchors live in the existing actor/body registry; there is no
//! second mutable portal pose. They are queryable but cannot be driven by input.
use super::*;
impl World {
    pub fn insert_anchor(&mut self, actor: Actor) -> Result<(), (WorldError, Box<Actor>)> {
        let failure =
            if actor.id.0 == 0 || self.contains_identity(actor.id) || self.anchors.len() >= 4096 {
                Some(WorldError::DuplicateActor)
            } else if let Some(shape) = actor.body.collision_shape() {
                self.geometry
                    .as_ref()
                    .ok_or(WorldError::MissingGeometry)
                    .and_then(|region| {
                        region
                            .validate_placement(
                                actor.cell.0,
                                actor.body.accepted().position(),
                                shape,
                                &[],
                                actor.id.0,
                            )
                            .map_err(PhysicsError::from)
                            .map_err(WorldError::from)
                    })
                    .err()
            } else {
                self.scene(actor.cell)
                    .and_then(|scene| {
                        actor
                            .body
                            .validate_placement(scene)
                            .map_err(WorldError::from)
                    })
                    .err()
            };
        if let Some(error) = failure {
            return Err((error, Box::new(actor)));
        }
        self.anchors.insert(actor.id);
        self.actors.insert(actor.id, actor);
        self.visibility.invalidate();
        Ok(())
    }
    pub fn is_anchor(&self, id: EntityId) -> bool {
        self.anchors.contains(&id)
    }
    pub fn remove_anchor(&mut self, id: EntityId) -> Option<Actor> {
        if self.anchors.contains(&id) {
            self.remove(id)
        } else {
            None
        }
    }
}
