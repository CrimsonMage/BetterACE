use super::*;
impl Magic {
    pub(crate) fn register_spell(
        &mut self,
        spell: PreparedMagicSpell,
    ) -> Result<(), CastRejection> {
        let s = &spell.spell;
        if self.spells.len() >= 16384 {
            return Err(CastRejection::Capacity);
        }
        if s.id == 0
            || s.id > u32::from(u16::MAX)
            || s.power > i32::MAX as u32
            || !s.range_constant.is_finite()
            || s.range_constant < 0.0
            || !s.range_per_skill.is_finite()
            || s.range_per_skill < 0.0
            || spell.gestures.len() > 32
            || spell.gestures.iter().any(|g| {
                if let Some(chain) = &g.motion_chain {
                    chain.motion != g.gesture.motion || chain.speed != 2.0
                } else {
                    !g.duration_seconds.is_finite()
                        || g.duration_seconds <= 0.0
                        || g.duration_seconds >= 4.0
                }
            })
            || spell.components.len() > 64
            || !spell.component_loss.is_finite()
            || spell.component_loss < 0.0
            || spell.component_modifiers.len() != spell.components.len()
            || spell
                .components
                .iter()
                .any(|(id, count)| *id == 0 || *count == 0)
        {
            return Err(CastRejection::InvalidState);
        }
        validate_effect(&s.effect)?;
        let mut count = 0_u32;
        for (index, (template, amount)) in spell.components.iter().enumerate() {
            count = count
                .checked_add(*amount)
                .ok_or(CastRejection::InvalidState)?;
            if count > 1024
                || spell.component_modifiers[index].0 != *template
                || !spell.component_modifiers[index].1.is_finite()
                || spell.component_modifiers[index].1 < 0.0
            {
                return Err(CastRejection::InvalidState);
            }
        }
        if self.spells.contains_key(&s.id) {
            return Err(CastRejection::InvalidState);
        }
        self.spells.insert(s.id, Arc::new(spell));
        Ok(())
    }
    pub(crate) fn register_caster(
        &mut self,
        actor: EntityId,
        caster: MagicCaster,
        world: &World,
    ) -> Result<(), CastRejection> {
        if self.casters.len() >= 4096 {
            return Err(CastRejection::Capacity);
        }
        if self.casters.contains_key(&actor)
            || caster.known_spells.len() > 8192
            || caster.school_skills.iter().any(|v| *v > i32::MAX as u32)
        {
            return Err(CastRejection::InvalidState);
        }
        world
            .actor_state(actor)
            .map_err(|_| CastRejection::MissingActor)?;
        world
            .vital(actor, EntityVital::Mana)
            .map_err(|_| CastRejection::MissingAssets)?;
        if !self.registries.contains_key(&actor) {
            let registry = EnchantmentRegistry::new(512).map_err(|_| CastRejection::Capacity)?;
            self.register_registry(actor, registry, true, self.current_time)
                .map_err(|(error, _)| error)?;
        }
        self.recovery.entry(actor).or_default();
        self.casters.insert(actor, caster);
        Ok(())
    }
}
impl Magic {
    /// Derived validation projection only. The character owner has already
    /// validated and durably adopted the bounded canonical spellbook.
    pub(crate) fn adopt_known_spell(&mut self, actor: EntityId, spell: u32) {
        if let Some(caster) = self.casters.get_mut(&actor) {
            caster.known_spells.insert(spell);
        }
    }
}

impl Magic {
    pub(crate) fn register_projectile_shape(
        &mut self,
        template: u32,
        shape: Arc<bace_physics::CollisionShape>,
    ) -> Result<(), CastRejection> {
        if template == 0
            || self.projectile_shapes.contains_key(&template)
            || self.projectile_shapes.len() >= 4096
        {
            return Err(CastRejection::InvalidState);
        }
        if shape.nominal_radius().is_none()
            || shape.nominal_height().is_none()
            || shape.spheres().len() != 1
            || shape.spheres()[0].center != Vec3::ZERO
            || shape.obstacles().any(|(_, height)| height.is_some())
        {
            return Err(CastRejection::MissingAssets);
        }
        self.projectile_shapes.insert(template, shape);
        Ok(())
    }
}
