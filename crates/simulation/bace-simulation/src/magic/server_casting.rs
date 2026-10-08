//! GDLE CreatureBeginCast and CastSpellInstant are separate from player formula
//! admission: creature mana is debited up front; only players fizzle/convert mana.
use super::*;
pub(super) fn creature_origin(origin: CastOrigin) -> bool {
    matches!(
        origin,
        CastOrigin::Monster { .. } | CastOrigin::Emote { instant: false, .. }
    )
}
impl Magic {
    pub(crate) fn register_server_program(
        &mut self,
        actor: EntityId,
        definition: PreparedMagicDefinition,
        shapes: Vec<(u32, Arc<bace_physics::CollisionShape>)>,
        uses_mana: bool,
    ) -> Result<(), CastRejection> {
        self.register_actor_program(actor, definition, shapes)?;
        self.server_mana.insert(actor, uses_mana);
        Ok(())
    }
    pub(crate) fn register_instant_definition(
        &mut self,
        definition: PreparedMagicDefinition,
        shapes: Vec<(u32, Arc<bace_physics::CollisionShape>)>,
    ) -> Result<(), CastRejection> {
        self.register_instant_batch(PreparedMagicAssetBatch {
            definitions: vec![definition],
            projectile_shapes: shapes,
        })
    }
    pub(crate) fn validate_instant_batch(
        &self,
        batch: &PreparedMagicAssetBatch,
    ) -> Result<(), CastRejection> {
        self.normalized_instant_batch(batch).map(|_| ())
    }
    pub(crate) fn register_instant_batch(
        &mut self,
        batch: PreparedMagicAssetBatch,
    ) -> Result<(), CastRejection> {
        let categories: Vec<_> = batch
            .definitions
            .iter()
            .map(|d| (d.spell.spell.id, d.category))
            .collect();
        let batch = self.normalized_instant_batch(&batch)?;
        let ids: Vec<_> = batch.definitions.iter().map(|d| d.spell.spell.id).collect();
        self.register_asset_batch(batch)?;
        self.actor_program_spells.extend(ids);
        self.spell_categories.extend(categories);
        Ok(())
    }
    fn normalized_instant_batch(
        &self,
        batch: &PreparedMagicAssetBatch,
    ) -> Result<PreparedMagicAssetBatch, CastRejection> {
        if batch.definitions.len() > 64 || batch.projectile_shapes.len() > 64 {
            return Err(CastRejection::Capacity);
        }
        let mut all = BTreeSet::new();
        let mut definitions = Vec::new();
        for definition in &batch.definitions {
            let id = definition.spell.spell.id;
            if !all.insert(id)
                || !definition.spell.gestures.is_empty()
                || !definition.spell.components.is_empty()
            {
                return Err(CastRejection::InvalidState);
            }
            if let Some(existing) = self.spells.get(&id) {
                if existing.spell != definition.spell.spell
                    || existing.fast_resistable_pk_spell
                        != definition.spell.fast_resistable_pk_spell
                    || self.enchantment_metadata.get(&id).copied() != definition.metadata
                    || self
                        .spell_categories
                        .get(&id)
                        .is_some_and(|old| *old != definition.category)
                    || self.damage_spell_levels.get(&id) != Some(&definition.formula_level)
                    || self.damage_spell_flags.get(&id) != Some(&definition.flags)
                    || self.spell_target_masks.get(&id) != Some(&definition.target_mask)
                {
                    return Err(CastRejection::InvalidState);
                }
            } else {
                definitions.push(definition.clone());
            }
        }
        let mut shapes = BTreeMap::new();
        for (id, shape) in &batch.projectile_shapes {
            let shape = if let Some(existing) = self.projectile_shapes.get(id) {
                if !actor_program::same_shape(existing, shape) {
                    return Err(CastRejection::MissingAssets);
                }
                existing.clone()
            } else {
                shape.clone()
            };
            if shapes.insert(*id, shape).is_some() {
                return Err(CastRejection::InvalidState);
            }
        }
        for definition in &batch.definitions {
            if let SpellEffect::Projectile(p) | SpellEffect::LifeProjectile { projectile: p, .. } =
                &definition.spell.spell.effect
                && let std::collections::btree_map::Entry::Vacant(entry) = shapes.entry(p.template)
            {
                entry.insert(
                    self.projectile_shapes
                        .get(&p.template)
                        .cloned()
                        .ok_or(CastRejection::MissingAssets)?,
                );
            }
        }
        let shapes: Vec<_> = shapes.into_iter().collect();
        let mut candidate = Magic::new(1);
        candidate.register_asset_batch(PreparedMagicAssetBatch {
            definitions: batch.definitions.clone(),
            projectile_shapes: shapes.clone(),
        })?;
        if self.spells.len() + definitions.len() > 16384
            || self.projectile_shapes.len()
                + shapes
                    .iter()
                    .filter(|(id, _)| !self.projectile_shapes.contains_key(id))
                    .count()
                > 4096
        {
            return Err(CastRejection::Capacity);
        }
        Ok(PreparedMagicAssetBatch {
            definitions,
            projectile_shapes: shapes,
        })
    }
    pub(super) fn prepare_creature_mana(
        &self,
        origin: CastOrigin,
        cost: u32,
        world: &World,
    ) -> Result<Option<VitalMutation>, CastRejection> {
        if !creature_origin(origin)
            || !self
                .server_mana
                .get(&origin.actor())
                .copied()
                .unwrap_or(true)
            || cost == 0
        {
            return Ok(None);
        }
        let before = world
            .vital(origin.actor(), EntityVital::Mana)
            .map_err(|_| CastRejection::MissingActor)?
            .current;
        let after = before
            .checked_sub(cost)
            .ok_or(CastRejection::InsufficientMana)?;
        let change = VitalMutation {
            actor: origin.actor(),
            vital: EntityVital::Mana,
            before,
            after,
        };
        world
            .validate_vital_batch(&[change], None)
            .map_err(|_| CastRejection::Busy)?;
        if self.events.len() + 2 > self.capacity {
            return Err(CastRejection::Capacity);
        }
        Ok(Some(change))
    }
}
impl Magic {
    pub(crate) fn validate_npc_proc_batches(
        &self,
        assets: &[(EntityId, Arc<crate::PreparedNpcCombatAssets>)],
    ) -> Result<(), CastRejection> {
        let mut definitions: BTreeMap<u32, &PreparedMagicDefinition> = BTreeMap::new();
        let mut shapes: BTreeMap<u32, &bace_physics::CollisionShape> = BTreeMap::new();
        for (_, input) in assets {
            let required = bace_magic::required_proc_spells(&input.damage, &input.physical)
                .map_err(|_| CastRejection::InvalidState)?;
            let supplied: BTreeSet<_> = input
                .server_magic
                .definitions
                .iter()
                .map(|d| d.spell.spell.id)
                .collect();
            if required.into_iter().collect::<BTreeSet<_>>() != supplied {
                return Err(CastRejection::MissingAssets);
            }
            self.validate_instant_batch(&input.server_magic)?;
            for definition in &input.server_magic.definitions {
                let id = definition.spell.spell.id;
                if let Some(old) = definitions.insert(id, definition)
                    && (old.spell.spell != definition.spell.spell
                        || old.metadata != definition.metadata
                        || old.flags != definition.flags
                        || old.formula_level != definition.formula_level
                        || old.category != definition.category
                        || old.target_mask != definition.target_mask
                        || old.spell.fast_resistable_pk_spell
                            != definition.spell.fast_resistable_pk_spell)
                {
                    return Err(CastRejection::InvalidState);
                }
            }
            for (id, shape) in &input.server_magic.projectile_shapes {
                if let Some(old) = shapes.insert(*id, shape)
                    && !actor_program::same_shape(old, shape)
                {
                    return Err(CastRejection::InvalidState);
                }
            }
        }
        if self.spells.len()
            + definitions
                .keys()
                .filter(|id| !self.spells.contains_key(id))
                .count()
            > 16384
            || self.projectile_shapes.len()
                + shapes
                    .keys()
                    .filter(|id| !self.projectile_shapes.contains_key(id))
                    .count()
                > 4096
        {
            return Err(CastRejection::Capacity);
        }
        Ok(())
    }
}
