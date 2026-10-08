//! Account formulas and avatar motions are actor-bound; spell effects remain shared.
use super::*;
#[derive(Clone, Debug)]
pub struct PreparedActorMagicProgram {
    pub portal_templates: Vec<(
        bace_interactions::PortalTemplate,
        Arc<bace_physics::CollisionShape>,
    )>,
    pub destination_cells: Vec<CellId>,
    pub binding: bace_gameplay_api::CharacterBinding,
    pub expected_character_revision: u64,
    pub definition: PreparedMagicDefinition,
    pub projectile_shapes: Vec<(u32, Arc<bace_physics::CollisionShape>)>,
}
impl Magic {
    pub(crate) fn invalidate_actor_programs(&mut self, actor: EntityId) {
        self.actor_components.retain(|(id, _), _| *id != actor);
        self.validated_actor_programs.retain(|(id, _)| *id != actor);
    }

    pub(crate) fn prepared_for_actor(
        &self,
        actor: EntityId,
        spell: u32,
    ) -> Result<Arc<PreparedMagicSpell>, CastRejection> {
        if self.actor_program_spells.contains(&spell)
            && !self.validated_actor_programs.contains(&(actor, spell))
        {
            return Err(CastRejection::MissingAssets);
        }
        if let Some(program) = self.actor_components.get(&(actor, spell)) {
            return Ok(program.clone());
        }
        self.spells
            .get(&spell)
            .cloned()
            .ok_or(CastRejection::UnknownSpell)
    }
    pub(crate) fn register_actor_program(
        &mut self,
        actor: EntityId,
        definition: PreparedMagicDefinition,
        mut shapes: Vec<(u32, Arc<bace_physics::CollisionShape>)>,
    ) -> Result<(), CastRejection> {
        let id = definition.spell.spell.id;
        let category = definition.category;
        if !self.casters.contains_key(&actor) {
            return Err(CastRejection::MissingActor);
        }
        if self.busy(actor) || self.registry_reserved(actor) {
            return Err(CastRejection::Busy);
        }
        if shapes.len() > 4096
            || self.actor_components.len() >= 65536
                && !self.actor_components.contains_key(&(actor, id))
        {
            return Err(CastRejection::Capacity);
        }
        if definition.spell.gestures.iter().any(|g| {
            g.motion_chain.is_none()
                || !g.gesture.minimum_seconds.is_finite()
                || !(0.0..=8.0).contains(&g.gesture.minimum_seconds)
        }) {
            return Err(CastRejection::MissingAssets);
        }
        let existing = self.spells.get(&id);
        if let Some(existing) = existing
            && (existing.spell != definition.spell.spell
                || existing.fast_resistable_pk_spell != definition.spell.fast_resistable_pk_spell
                || self.enchantment_metadata.get(&id).copied() != definition.metadata
                || self
                    .spell_categories
                    .get(&id)
                    .is_some_and(|old| *old != definition.category)
                || self.damage_spell_levels.get(&id) != Some(&definition.formula_level)
                || self.damage_spell_flags.get(&id) != Some(&definition.flags)
                || self.spell_target_masks.get(&id) != Some(&definition.target_mask))
        {
            return Err(CastRejection::InvalidState);
        }
        for (template, shape) in &mut shapes {
            if let Some(current) = self.projectile_shapes.get(template) {
                if !same_shape(current, shape) {
                    return Err(CastRejection::MissingAssets);
                }
                *shape = current.clone();
            }
        }
        // Validate the complete isolated candidate before publishing any index.
        let mut candidate = Magic::new(1);
        candidate.register_asset_batch(PreparedMagicAssetBatch {
            definitions: vec![definition],
            projectile_shapes: shapes,
        })?;
        if existing.is_none() && self.spells.len() >= 16384
            || self.projectile_shapes.len()
                + candidate
                    .projectile_shapes
                    .keys()
                    .filter(|id| !self.projectile_shapes.contains_key(id))
                    .count()
                > 4096
        {
            return Err(CastRejection::Capacity);
        }
        let prepared = candidate
            .spells
            .remove(&id)
            .expect("validated native definition");
        if existing.is_none() {
            self.spells.insert(id, prepared.clone());
            self.enchantment_metadata
                .append(&mut candidate.enchantment_metadata);
            self.damage_spell_levels
                .append(&mut candidate.damage_spell_levels);
            self.damage_spell_flags
                .append(&mut candidate.damage_spell_flags);
            self.spell_target_masks
                .append(&mut candidate.spell_target_masks);
            self.spell_categories
                .append(&mut candidate.spell_categories);
        }
        self.projectile_shapes
            .append(&mut candidate.projectile_shapes);
        self.spell_categories.entry(id).or_insert(category);
        self.actor_components.insert((actor, id), prepared);
        self.actor_program_spells.insert(id);
        self.validated_actor_programs.insert((actor, id));
        Ok(())
    }
}
pub(super) fn same_shape(
    a: &bace_physics::CollisionShape,
    b: &bace_physics::CollisionShape,
) -> bool {
    a.spheres() == b.spheres()
        && a.step_up == b.step_up
        && a.step_down == b.step_down
        && a.nominal_radius() == b.nominal_radius()
        && a.nominal_height() == b.nominal_height()
        && a.cylinders().len() == b.cylinders().len()
        && a.cylinders()
            .iter()
            .zip(b.cylinders())
            .all(|(a, b)| a.base == b.base && a.radius == b.radius && a.height == b.height)
}
#[cfg(test)]
mod tests;
