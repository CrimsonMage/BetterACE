//! Atomic cold publication of validated spell, formula, target and shape indexes.
use super::*;
#[derive(Clone, Debug)]
pub struct PreparedMagicDefinition {
    pub spell: PreparedMagicSpell,
    pub metadata: Option<EnchantmentMetadata>,
    pub formula_level: u32,
    pub category: u32,
    pub flags: u32,
    pub target_mask: u32,
}
#[derive(Clone, Debug)]
pub struct PreparedMagicAssetBatch {
    pub definitions: Vec<PreparedMagicDefinition>,
    pub projectile_shapes: Vec<(u32, Arc<bace_physics::CollisionShape>)>,
}
impl Magic {
    pub(crate) fn register_asset_batch(
        &mut self,
        batch: PreparedMagicAssetBatch,
    ) -> Result<(), CastRejection> {
        if batch.definitions.len() > 16384usize.saturating_sub(self.spells.len())
            || batch.projectile_shapes.len() > 4096
        {
            return Err(CastRejection::Capacity);
        }
        let mut candidate = Magic::new(self.capacity);
        for definition in batch.definitions {
            let id = definition.spell.spell.id;
            if self.spells.contains_key(&id) {
                return Err(CastRejection::InvalidState);
            }
            candidate.register_spell(definition.spell)?;
            candidate.register_damage_spell(id, definition.formula_level)?;
            if definition.category > 65535 {
                return Err(CastRejection::InvalidState);
            }
            candidate.spell_categories.insert(id, definition.category);
            candidate.register_damage_spell_flags(id, definition.flags)?;
            candidate.register_spell_target_mask(id, definition.target_mask)?;
            if let Some(metadata) = definition.metadata {
                candidate.register_enchantment_metadata(id, metadata)?;
            }
        }
        for (template, shape) in batch.projectile_shapes {
            if let Some(existing) = self.projectile_shapes.get(&template) {
                if !Arc::ptr_eq(existing, &shape) {
                    return Err(CastRejection::InvalidState);
                }
                continue;
            }
            candidate.register_projectile_shape(template, shape)?;
        }
        if self.projectile_shapes.len() + candidate.projectile_shapes.len() > 4096 {
            return Err(CastRejection::Capacity);
        }
        for spell in candidate.spells.values() {
            match &spell.spell.effect {
                SpellEffect::Projectile(p) | SpellEffect::LifeProjectile { projectile: p, .. } => {
                    let shape = candidate
                        .projectile_shapes
                        .get(&p.template)
                        .or_else(|| self.projectile_shapes.get(&p.template))
                        .ok_or(CastRejection::MissingAssets)?;
                    if shape.nominal_radius() != Some(p.radius) {
                        return Err(CastRejection::MissingAssets);
                    }
                    if p.enchantment.is_some()
                        && !candidate.enchantment_metadata.contains_key(&spell.spell.id)
                    {
                        return Err(CastRejection::MissingAssets);
                    }
                }
                SpellEffect::Enchantment(_) | SpellEffect::FellowshipEnchantment(_)
                    if !candidate.enchantment_metadata.contains_key(&spell.spell.id) =>
                {
                    return Err(CastRejection::MissingAssets);
                }
                _ => {}
            }
        }
        self.spells.append(&mut candidate.spells);
        self.spell_categories
            .append(&mut candidate.spell_categories);
        self.enchantment_metadata
            .append(&mut candidate.enchantment_metadata);
        self.damage_spell_levels
            .append(&mut candidate.damage_spell_levels);
        self.damage_spell_flags
            .append(&mut candidate.damage_spell_flags);
        self.spell_target_masks
            .append(&mut candidate.spell_target_masks);
        self.projectile_shapes
            .append(&mut candidate.projectile_shapes);
        Ok(())
    }
}
