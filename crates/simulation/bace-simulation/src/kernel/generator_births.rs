//! Production generator births retain accepted renderer/PVS evidence before the
//! inventory/generator receipt commits. Synthetic harnesses opt out explicitly.
use super::*;
use crate::GeneratorServiceError;
pub(super) type Birth = Option<std::sync::Arc<bace_gameplay_api::visibility::AcceptedObjectBirth>>;
pub(super) type Births = std::collections::BTreeMap<EntityId, Birth>;
impl Kernel {
    pub(super) fn prepare_generator_birth(
        &mut self,
        entity: EntityId,
    ) -> Result<Birth, GeneratorServiceError> {
        if !self.generators.prepared_births {
            return Ok(None);
        }
        let characters = &self.characters;
        self.world
            .object_birth_snapshot(entity, self.tick, |actor| characters.entered_binding(actor))
            .map(|birth| Some(std::sync::Arc::new(birth)))
            .map_err(birth_error)
    }
    pub(super) fn prepare_staged_generator_birth(
        &mut self,
        actor: &bace_entity::Actor,
    ) -> Result<Birth, GeneratorServiceError> {
        if !self.generators.prepared_births {
            return Ok(None);
        }
        let characters = &self.characters;
        self.world
            .staged_object_birth_snapshot(actor, self.tick, |actor| {
                characters.entered_binding(actor)
            })
            .map(|birth| Some(std::sync::Arc::new(birth)))
            .map_err(birth_error)
    }
}
fn birth_error(error: bace_world::VisibilityError) -> GeneratorServiceError {
    match error {
        bace_world::VisibilityError::Capacity => GeneratorServiceError::Capacity,
        bace_world::VisibilityError::MissingActor => GeneratorServiceError::Missing,
        _ => GeneratorServiceError::Geometry,
    }
}
