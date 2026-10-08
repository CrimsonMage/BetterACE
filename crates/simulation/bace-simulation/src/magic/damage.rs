//! Owner-held damage preparation and live registry composition; no I/O.
use super::*;
impl Magic {
    /// Skills in order: magic defense, sneak, deception, assess person, shield.
    /// Shield uses base skill; the other four use current accepted skill.
    pub(crate) fn refresh_damage_skills(
        &mut self,
        actor: EntityId,
        skills: [bace_gameplay_api::weapon_combat::PhysicalSkill; 5],
        base_attributes: [u32; 2],
    ) -> Result<(), CastRejection> {
        if skills
            .iter()
            .any(|s| s.advancement > 3 || s.current > i32::MAX as u32)
        {
            return Err(CastRejection::InvalidState);
        }
        if let Some(profile) = self.damage_profiles.get_mut(&actor) {
            profile.magic_defense = skills[0];
            profile.sneak = skills[1];
            profile.deception = skills[2];
            profile.assess_person = skills[3];
            if let Some((shield, _)) = &mut profile.shield {
                *shield = skills[4];
            }
            profile.base_strength = base_attributes[0];
            profile.base_endurance = base_attributes[1];
        }
        Ok(())
    }
    pub(crate) fn register_damage_profile(
        &mut self,
        actor: EntityId,
        profile: bace_magic::MagicDamageProfile,
    ) -> Result<(), CastRejection> {
        if !self.registries.contains_key(&actor) {
            return Err(CastRejection::MissingActor);
        }
        if self.busy(actor) || self.registry_reserved(actor) {
            return Err(CastRejection::Busy);
        }
        bace_magic::validate_magic_damage_profile(&profile)
            .map_err(|_| CastRejection::InvalidState)?;
        self.publish_damage_profile(actor, profile);
        Ok(())
    }
    pub(crate) fn refresh_damage_profile(
        &mut self,
        actor: EntityId,
        mut profile: bace_magic::MagicDamageProfile,
    ) -> Result<(), CastRejection> {
        bace_magic::validate_magic_damage_profile(&profile)
            .map_err(|_| CastRejection::InvalidState)?;
        let previous = self
            .damage_profiles
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?;
        if previous.player != profile.player {
            return Err(CastRejection::InvalidState);
        }
        if profile.wand.as_ref().is_some_and(|w| {
            self.damage_wands
                .get(&w.entity)
                .is_some_and(|(old, _)| old.revision > w.revision)
        }) {
            return Err(CastRejection::InvalidState);
        }
        if self.registry_reserved(actor) {
            return Err(CastRejection::Busy);
        }
        profile.current_enemy = previous.current_enemy;
        self.publish_damage_profile(actor, profile);
        Ok(())
    }
    pub(super) fn publish_damage_profile(
        &mut self,
        actor: EntityId,
        profile: bace_magic::MagicDamageProfile,
    ) {
        if let Some(previous) = self
            .damage_profiles
            .get(&actor)
            .and_then(|p| p.wand.as_ref())
            && profile
                .wand
                .as_ref()
                .is_none_or(|w| w.entity != previous.entity)
            && let Some((_, owner)) = self.damage_wands.get_mut(&previous.entity)
            && *owner == Some(actor)
        {
            *owner = None;
        }
        if let Some(wand) = &profile.wand {
            self.damage_wands
                .insert(wand.entity, (wand.clone(), Some(actor)));
        }
        self.damage_profiles.insert(actor, profile);
        self.damage_wands.retain(|id, _| {
            self.damage_profiles
                .values()
                .any(|p| p.wand.as_ref().is_some_and(|w| w.entity == *id))
                || self
                    .flying
                    .values()
                    .any(|f| f.launch_wand.as_ref().is_some_and(|w| w.entity == *id))
        });
    }
    /// Accepted cold item mutation for a wand still referenced by an in-flight
    /// projectile, including a wand currently unequipped or owned by another actor.
    pub(crate) fn references_damage_wand(&self, item: EntityId) -> bool {
        self.damage_wands.contains_key(&item.0)
    }
    pub(crate) fn refresh_damage_wand(
        &mut self,
        wand: bace_magic::MagicWand,
    ) -> Result<(), CastRejection> {
        let mut profile = bace_magic::MagicDamageProfile::neutral(false);
        profile.wand = Some(wand.clone());
        bace_magic::validate_magic_damage_profile(&profile)
            .map_err(|_| CastRejection::InvalidState)?;
        if let Some((previous, _)) = self.damage_wands.get_mut(&wand.entity) {
            if wand.revision < previous.revision {
                return Err(CastRejection::InvalidState);
            }
            *previous = wand;
        }
        Ok(())
    }
    pub(crate) fn register_damage_spell(
        &mut self,
        spell: u32,
        formula_level: u32,
    ) -> Result<(), CastRejection> {
        if formula_level > 8 || !self.spells.contains_key(&spell) {
            return Err(CastRejection::InvalidState);
        }
        if self.attempts.values().any(|a| a.prepared.spell.id == spell)
            || self.flying.values().any(|a| a.spell.spell.id == spell)
        {
            return Err(CastRejection::Busy);
        }
        self.damage_spell_levels.insert(spell, formula_level);
        Ok(())
    }
    pub(super) fn validate_damage_preparation(
        &self,
        actor: EntityId,
        target: Option<EntityId>,
        spell: &PreparedSpell,
        world: &World,
    ) -> Result<(), CastRejection> {
        if !matches!(
            spell.effect,
            SpellEffect::Projectile(_)
                | SpellEffect::LifeProjectile { .. }
                | SpellEffect::Boost { .. }
                | SpellEffect::Transfer { .. }
        ) {
            return Ok(());
        }
        let formula_needed = matches!(
            spell.effect,
            SpellEffect::Projectile(_)
                | SpellEffect::LifeProjectile { .. }
                | SpellEffect::Boost {
                    minimum: i32::MIN..=-1,
                    ..
                }
        );
        let authentic = world
            .body(actor)
            .is_ok_and(|b| b.collision_shape().is_some());
        if authentic
            && (!self.damage_profiles.contains_key(&actor)
                || formula_needed && !self.damage_spell_levels.contains_key(&spell.id))
        {
            return Err(CastRejection::MissingAssets);
        }
        if let Some(target) = target
            && (self.damage_profiles.contains_key(&actor)
                || world
                    .body(target)
                    .is_ok_and(|b| b.collision_shape().is_some()))
            && !self.damage_profiles.contains_key(&target)
        {
            return Err(CastRejection::MissingAssets);
        }
        if formula_needed
            && self.damage_profiles.contains_key(&actor)
            && !self.damage_spell_levels.contains_key(&spell.id)
        {
            return Err(CastRejection::MissingAssets);
        }
        Ok(())
    }
    pub(super) fn live_damage_profile(
        &self,
        actor: EntityId,
    ) -> Result<bace_magic::MagicDamageProfile, CastRejection> {
        let mut profile = self
            .damage_profiles
            .get(&actor)
            .cloned()
            .ok_or(CastRejection::MissingAssets)?;
        if let Some(defense) = self.defense_profiles.get(&actor) {
            profile.magic_defense = bace_gameplay_api::weapon_combat::PhysicalSkill {
                advancement: match defense.magic_defense.advancement {
                    bace_gameplay_api::SkillAdvancement::Inactive => 0,
                    bace_gameplay_api::SkillAdvancement::Untrained => 1,
                    bace_gameplay_api::SkillAdvancement::Trained => 2,
                    bace_gameplay_api::SkillAdvancement::Specialized => 3,
                },
                current: defense.magic_defense.current,
            };
        }
        if let Some(registry) = self.registries.get(&actor) {
            for (index, key) in [64, 65, 66, 67, 68, 69, 70, 166, 125, 72, 74]
                .into_iter()
                .enumerate()
            {
                profile.resistances[index].quality = bace_magic::enchant_physical_quality(
                    registry,
                    8,
                    key,
                    profile.resistances[index].quality.raw,
                    false,
                )
                .map_err(|_| CastRejection::InvalidState)?
                .0;
            }
            if !profile.rating_properties.is_empty() {
                for (key, value) in &mut profile.rating_properties {
                    *value = bace_magic::enchant_physical_quality(
                        registry,
                        4,
                        *key,
                        f64::from(*value),
                        true,
                    )
                    .map_err(|_| CastRejection::InvalidState)?
                    .1 as i32;
                }
                let prop = |id| {
                    profile
                        .rating_properties
                        .iter()
                        .find(|p| p.0 == id)
                        .map_or(0, |p| p.1)
                };
                profile.healing_ratings = [
                    prop(323)
                        .checked_add(prop(376))
                        .and_then(|v| v.checked_add(prop(342)))
                        .ok_or(CastRejection::InvalidState)?,
                    prop(317),
                    prop(351),
                ];
                profile.ratings = bace_magic::magic_ratings(&profile.rating_properties)
                    .map_err(|_| CastRejection::InvalidState)?;
            }
        }
        if let Some(registry) = self.registries.get(&actor) {
            for (index, key) in [71, 73, 75].into_iter().enumerate() {
                profile.boost_resistances[index] = bace_magic::enchant_physical_quality(
                    registry,
                    8,
                    key,
                    profile.boost_resistances[index],
                    false,
                )
                .map_err(|_| CastRejection::InvalidState)?
                .1;
            }
        }
        if let (Some(entity), Some((_, cap))) = (profile.shield_entity, profile.shield.as_mut())
            && let Some(registry) = self.registries.get(&EntityId(entity))
        {
            *cap = bace_magic::enchant_physical_quality(registry, 8, 159, f64::from(*cap), false)
                .map_err(|_| CastRejection::InvalidState)?
                .1 as f32;
        }
        Ok(profile)
    }
    pub(super) fn compose_damage_wand(
        &self,
        _actor: EntityId,
        source: &mut bace_magic::MagicDamageProfile,
        inherit: bool,
    ) -> Result<(), CastRejection> {
        if !inherit
            && let Some(wand) = source.wand.as_mut()
            && !wand.elemental_present
        {
            wand.elemental_modifier = 1.;
        }
        if let Some(wand) = source.wand.as_mut() {
            if let Some(registry) = self.registries.get(&EntityId(wand.entity)) {
                for (key, value) in [
                    (152, &mut wand.elemental_modifier),
                    (147, &mut wand.biting),
                    (136, &mut wand.crushing),
                    (138, &mut wand.slayer_bonus),
                ] {
                    *value = bace_magic::enchant_physical_quality(registry, 8, key, *value, false)
                        .map_err(|_| CastRejection::InvalidState)?
                        .1;
                }
            }
            if let Some(registry) = self.registries.get(&EntityId(wand.entity)) {
                // GetBitingStrikeFrequency/GetCrushingBlowMultiplier first
                // query the enchanted stored quality, then explicitly enchant
                // once more. An absent property only takes the latter pass.
                if wand.double_enchant_biting {
                    wand.biting =
                        bace_magic::enchant_physical_quality(registry, 8, 147, wand.biting, false)
                            .map_err(|_| CastRejection::InvalidState)?
                            .1;
                }
                if wand.double_enchant_crushing {
                    wand.crushing = bace_magic::enchant_physical_quality(
                        registry,
                        8,
                        136,
                        wand.crushing,
                        false,
                    )
                    .map_err(|_| CastRejection::InvalidState)?
                    .1;
                }
            }
            if inherit && wand.inherit_wielder {
                let wielder = self
                    .damage_wands
                    .get(&wand.entity)
                    .and_then(|(_, owner)| *owner);
                let owner_profile = wielder.and_then(|owner| self.damage_profiles.get(&owner));
                let mut elemental = owner_profile.map_or(0., |p| p.elemental_modifier);
                if let Some(owner) = wielder
                    && let Some(registry) = self.registries.get(&owner)
                {
                    elemental =
                        bace_magic::enchant_physical_quality(registry, 8, 152, elemental, false)
                            .map_err(|_| CastRejection::InvalidState)?
                            .1;
                    if owner_profile.is_some_and(|p| p.player)
                        && !bace_magic::enchantment_modifiers(registry, 8, 170).is_empty()
                    {
                        elemental =
                            bace_magic::enchant_physical_quality(registry, 8, 170, 0., false)
                                .map_err(|_| CastRejection::InvalidState)?
                                .1;
                    }
                }
                wand.elemental_modifier += elemental;
            }
        }
        Ok(())
    }
    pub(super) fn resolved_projectile_damage(
        &self,
        flying: &Flying,
        target: EntityId,
        projectile: EntityId,
        spec: &ProjectileSpec,
        random: &mut RandomStream,
        world: &World,
    ) -> Result<u32, CastRejection> {
        if !self.damage_profiles.contains_key(&flying.source) {
            if world
                .body(flying.source)
                .is_ok_and(|b| b.collision_shape().is_some())
                || world
                    .body(target)
                    .is_ok_and(|b| b.collision_shape().is_some())
            {
                return Err(CastRejection::MissingAssets);
            }
            let damage = if let Some(base) = flying.life_damage {
                base.round() as u32
            } else {
                roll_range(
                    random,
                    spec.minimum_damage as i32,
                    spec.maximum_damage as i32,
                )? as u32
            };
            return self.projectile_damage(flying.source, target, projectile, damage, world);
        }
        let mut source = self.live_damage_profile(flying.source)?;
        source.wand = flying.launch_wand.as_ref().and_then(|wand| {
            self.damage_wands
                .get(&wand.entity)
                .map(|(current, _)| current.clone())
        });
        self.compose_damage_wand(flying.source, &mut source, true)?;
        let defender = self.live_damage_profile(target)?;
        let (_, target_state) = world
            .actor_state(target)
            .map_err(|_| CastRejection::InvalidTarget)?;
        let (cell, _) = world
            .actor_state(target)
            .map_err(|_| CastRejection::InvalidTarget)?;
        let (source_position, _) = world
            .actor_in_frame(flying.source, cell)
            .map_err(|_| CastRejection::MissingAssets)?;
        let offset = source_position - target_state.position();
        let angle = heading_delta(target_state.heading_radians(), (-offset.x).atan2(offset.y))
            .map_err(|_| CastRejection::InvalidState)?
            .to_degrees();
        let input = bace_magic::MagicDamageInput {
            source: &source,
            target: &defender,
            school: flying.spell.spell.school,
            skill: flying.cast_skill,
            formula_level: *self
                .damage_spell_levels
                .get(&flying.spell.spell.id)
                .ok_or(CastRejection::MissingAssets)?,
            damage_type: spec.damage_type,
            minimum: spec.minimum_damage,
            maximum: spec.maximum_damage,
            life_damage: flying.life_damage.map(f64::from),
            projectile: true,
            target_in_combat: !accepted_peace_style(world, target),
            target_angle_degrees: f64::from(angle),
            rolls: bace_magic::MagicDamageRolls {
                variance: f64::from(unit(random)?),
                critical: f64::from(unit(random)?),
                critical_defense: unit(random)?,
                sneak: unit(random)?,
            },
        };
        let before = bace_magic::magic_damage_before_mitigation(&input)
            .map_err(|_| CastRejection::InvalidState)?;
        let modifier = bace_combat::physical::physical_rating_modifier(before.rating)
            .map_err(|_| CastRejection::InvalidState)?;
        bace_magic::magic_damage_after_mitigation(&input, before, modifier)
            .map_err(|_| CastRejection::InvalidState)
    }
}
