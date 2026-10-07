use bace_entity::Actor;
use bace_geometry::Vec3;
use bace_physics::{AcceptedState, Body, PhysicsError, SyntheticScene};
use bace_types::{CellId, EntityId};
use std::collections::BTreeMap;

/// Single-owner synthetic world harness. This is not an AC landblock loader.
#[derive(Default)]
pub struct World {
    scenes: BTreeMap<CellId, SyntheticScene>,
    actors: BTreeMap<EntityId, Actor>,
}

impl World {
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
        actor.body.validate_placement(self.scene(actor.cell)?)?;
        if self.actors.contains_key(&actor.id) {
            return Err(WorldError::DuplicateActor);
        }
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
        for actor in self.actors.values_mut() {
            let scene = self
                .scenes
                .get(&actor.cell)
                .ok_or(WorldError::MissingGeometry)?;
            actor.body.step(scene);
        }
        Ok(())
    }
    pub fn teleport(
        &mut self,
        id: EntityId,
        destination: CellId,
        position: Vec3,
    ) -> Result<(), WorldError> {
        let scene = self
            .scenes
            .get(&destination)
            .ok_or(WorldError::MissingGeometry)?;
        let actor = self.actors.get_mut(&id).ok_or(WorldError::MissingActor)?;
        actor.body.server_teleport(scene, position)?;
        actor.cell = destination;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("cell has no validated collision geometry")]
    MissingGeometry,
    #[error("scene already registered")]
    DuplicateScene,
    #[error("actor ID already owned")]
    DuplicateActor,
    #[error("actor does not exist")]
    MissingActor,
    #[error(transparent)]
    Physics(#[from] PhysicsError),
}
