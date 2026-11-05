use super::*;

impl Magic {
    pub(super) fn release(
        &mut self,
        attempt: &mut Attempt,
        world: &mut World,
        now: f64,
        policy: &Combat,
        fellowships: &crate::fellowships::Fellowships,
        observers: Option<(&crate::characters::Characters, u64)>,
    ) -> Result<bool, CastRejection> {
        if !self.health_output_ready(attempt, world, fellowships) {
            return Ok(false);
        }
        if attempt.pending_direct.is_some() {
            let resources = attempt
                .resources
                .as_ref()
                .ok_or(CastRejection::InvalidState)?;
            let cost = if attempt.mana_applied {
                0
            } else {
                resources.cost
            };
            let mut random = resources.random.clone();
            return self.release_direct_boost(attempt, world, cost, &mut random);
        }
        let observed_target = self.observed_target(attempt, world)?;
        let mut release_displaced = false;
        if !attempt.origin.instant() {
            let skill = attempt.cast_skill;
            let observed = observe(
                world,
                attempt.origin.actor(),
                observed_target,
                &attempt.prepared.spell,
                skill,
            )?;
            release_displaced = attempt.driver.movement_disrupted(observed);
            let signal = attempt
                .driver
                .revalidate_release(now, observed)
                .map_err(rejection)?;
            if !matches!(signal, CastSignal::Release { .. }) {
                self.signal(attempt, signal, world, now)?;
                return Ok(false);
            }
        }
        let prepared = attempt.prepared.clone();
        let spell = &prepared.spell;
        let source_beneficial = self
            .damage_spell_flags
            .get(&spell.id)
            .map_or_else(|| helpful_spell(spell), |flags| flags & 4 != 0);
        let actor = attempt.origin.actor();
        let target = attempt_target(attempt).unwrap_or(actor);
        if !matches!(
            spell.effect,
            SpellEffect::Portal(bace_magic::PortalEffect::Link { .. })
        ) && (!matches!(attempt.origin, CastOrigin::Staff { .. })
            || matches!(
                spell.effect,
                SpellEffect::Dispel(_) | SpellEffect::FellowshipDispel(_)
            ))
        {
            self.spell_permission(
                policy,
                actor,
                observed_target.unwrap_or(actor),
                helpful_spell(spell),
                world,
            )?;
        }
        if self
            .registry_clocks
            .get(&target)
            .is_some_and(|c| c.reserved || c.error.is_some())
        {
            return Ok(false);
        }
        if attempt.portal_request_sent {
            return Ok(false);
        }
        let needed = match &spell.effect {
            SpellEffect::Projectile(p) => usize::from(p.count) + 1,
            SpellEffect::LifeProjectile { projectile: p, .. } => usize::from(p.count) + 2,
            SpellEffect::Transfer { .. } => 3,
            _ => 2,
        };
        if self.events.len() + needed > self.capacity || self.combat.len() + 1 > self.capacity {
            return Ok(false);
        }
        let skill = attempt.cast_skill;
        if attempt.resources.is_none() {
            let mut random = attempt.random.clone();
            // CreatureBeginCast already charged base mana at admission. GDLE's
            // actual-player subtype check charges player-owned creature/emote
            // casts again; BetterACE intentionally keys release costs and
            // fizzle/conversion rules to the explicit player-cast origin.
            // See gdle-player-emote-double-mana in docs/divergences.toml.
            let uses_resources = matches!(attempt.origin, CastOrigin::Player(_));
            let school_locked = uses_resources
                && self
                    .recovery
                    .get(&actor)
                    .copied()
                    .unwrap_or_default()
                    .school_locked(spell.school, now, f64::from(unit(&mut random)?))
                    .map_err(rejection)?;
            let fizzled = uses_resources
                && f64::from(unit(&mut random)?)
                    > bace_magic::cast_chance(skill, spell.power)
                        .map_err(|_| CastRejection::InvalidState)?;
            // GDLE evaluates the skill roll first, then movement, before any
            // component burn. A retained paid resource plan never re-rolls this.
            if uses_resources && !fizzled && release_displaced {
                let signal = attempt
                    .driver
                    .movement_fizzle_at_release(attempt.cast, now)
                    .map_err(rejection)?;
                self.signal(attempt, signal, world, now)?;
                return Ok(false);
            }
            let cost = if !uses_resources {
                0
            } else if fizzled || school_locked {
                5
            } else {
                // ACE Creature.CalculateManaUsage returns base cost when the
                // spell carries IgnoresManaConversion (SpellFlags 0x800).
                // The caster's skill is already zero for an untrained owner.
                let (base_mana, conversion) =
                    self.mana_conversion_inputs(actor, spell.id, spell.base_mana, world)?;
                if conversion == 0 {
                    base_mana
                } else {
                    // The pinned ACE GetManaCost draws roll/luck, then draws a
                    // second roll only when the first pass left cost above 1.
                    let first = unit(&mut random)?;
                    let luck = unit(&mut random)?;
                    match bace_magic::mana_cost(spell.power, base_mana, conversion, &[first, luck])
                    {
                        Ok((cost, _)) => cost,
                        Err(bace_magic::MagicFormulaError::MissingDraw) => {
                            let second = unit(&mut random)?;
                            bace_magic::mana_cost(
                                spell.power,
                                base_mana,
                                conversion,
                                &[first, luck, second],
                            )
                            .map_err(|_| CastRejection::InvalidState)?
                            .0
                        }
                        Err(_) => return Err(CastRejection::InvalidState),
                    }
                }
            };
            let mut consumed = Vec::new();
            if matches!(attempt.origin, CastOrigin::Player(_))
                && !self.casters[&actor].safe_components
            {
                let mut components = random
                    .fork(b"components", 0)
                    .map_err(|_| CastRejection::InvalidState)?;
                for ((template, amount), (_, modifier)) in attempt
                    .prepared
                    .components
                    .iter()
                    .zip(&attempt.prepared.component_modifiers)
                {
                    let rate = bace_magic::component_burn_rate(
                        attempt.prepared.component_loss,
                        *modifier,
                        spell.power,
                        skill,
                    )
                    .map_err(|_| CastRejection::InvalidState)?;
                    let mut burn = 0;
                    for _ in 0..*amount {
                        if unit(&mut components)? < rate {
                            burn += 1;
                        }
                    }
                    if burn != 0 {
                        consumed.push((*template, burn));
                    }
                }
            }
            attempt.resources = Some(ResourcePlan {
                cost,
                fizzled,
                school_locked,
                consumed,
                random,
            });
        }
        let resources = attempt.resources.as_ref().expect("prepared above");
        let cost = if attempt.mana_applied {
            0
        } else {
            resources.cost
        };
        let mut random = resources.random.clone();
        if cost > 0
            && world
                .vital(actor, EntityVital::Mana)
                .map_err(|_| CastRejection::MissingAssets)?
                .current
                < cost
        {
            return Err(CastRejection::InsufficientMana);
        }
        let mana_preview = self.prepare_vital_mutations(world, actor, cost, &[])?;
        world
            .validate_vital_batch(&mana_preview, Some(actor))
            .map_err(|_| CastRejection::InvalidState)?;
        if !attempt.components_confirmed && !attempt.prepared.components.is_empty() {
            if !attempt.component_request_sent {
                self.events.push_back(MagicEvent::ComponentsRequired {
                    cast: attempt.cast,
                    actor,
                    requirements: attempt.prepared.components.clone(),
                    consumed: resources.consumed.clone(),
                });
                attempt.component_request_sent = true;
            }
            return Ok(false);
        }

        attempt
            .driver
            .record_release_attempt(attempt.cast, now)
            .map_err(rejection)?;
        if resources.school_locked {
            self.apply_mana(world, actor, cost)?;
            attempt.random = random;
            return Err(CastRejection::SchoolRecovery);
        }
        if resources.fizzled {
            self.apply_mana(world, actor, cost)?;
            self.events.push_back(MagicEvent::Fizzle {
                actor,
                intensity: f32::from_bits(0x3f0af0a2),
                movement_incarnation: None,
            });
            attempt.random = random;
            return Err(CastRejection::Fizzled);
        }
        if matches!(spell.effect, SpellEffect::Boost{minimum,..} if minimum<0) {
            pk_activity::direct(world, policy, actor, target, now);
        }
        if spell.resistable
            && !matches!(attempt.origin, CastOrigin::Staff { .. })
            && target != actor
            && !matches!(
                spell.effect,
                SpellEffect::Projectile(_)
                    | SpellEffect::LifeProjectile { .. }
                    | SpellEffect::FellowshipBoost { .. }
                    | SpellEffect::FellowshipEnchantment(_)
                    | SpellEffect::FellowshipDispel(_)
            )
        {
            let defense = self
                .casters
                .get(&observed_target.unwrap_or(target))
                .map_or(0, |c| c.magic_defense);
            if bace_magic::resisted(skill, defense, unit(&mut random)?)
                .map_err(|_| CastRejection::InvalidState)?
                .0
                || world
                    .combatant(target)
                    .is_some_and(|c| c.lifestone_protected())
            {
                self.apply_mana(world, actor, cost)?;
                self.target_rejected(
                    actor,
                    observed_target.unwrap_or(target),
                    spell.id,
                    CastRejection::Resisted,
                    world,
                    observers,
                );
                attempt.random = random;
                return Ok(true);
            }
        }
        if attempt.item_target.is_some()
            && self
                .damage_spell_flags
                .get(&spell.id)
                .is_none_or(|flags| flags & 0x10000 == 0)
        {
            let (resistance, immune) = self
                .item_spell_qualities
                .get(&target)
                .copied()
                .unwrap_or((0, false));
            if immune
                || resistance >= 9999
                || resistance > 0
                    && bace_magic::resisted(skill, resistance, unit(&mut random)?)
                        .map_err(|_| CastRejection::InvalidState)?
                        .0
            {
                self.apply_mana(world, actor, cost)?;
                self.target_rejected(
                    actor,
                    target,
                    spell.id,
                    CastRejection::Resisted,
                    world,
                    observers,
                );
                attempt.random = random;
                return Ok(true);
            }
        }
        match &spell.effect {
            SpellEffect::Boost {
                vital,
                minimum,
                maximum,
            } => {
                if self.damage_profiles.contains_key(&actor) {
                    return self.release_direct_boost(attempt, world, cost, &mut random);
                }
                let roll = roll_range(&mut random, *minimum, *maximum)?;
                let current = vital_after_cost(world, target, entity_vital(*vital), actor, cost)?;
                let change = bace_magic::boost(
                    VitalState {
                        current: current.current,
                        maximum: current.maximum,
                    },
                    *minimum,
                    *maximum,
                    roll,
                    if roll > 0 {
                        f64::from(self.healing_amount_modifier(target))
                    } else {
                        1.0
                    },
                )
                .map_err(|_| CastRejection::InvalidState)?;
                self.commit_vitals(
                    world,
                    actor,
                    cost,
                    &[VitalMutation {
                        actor: target,
                        vital: entity_vital(*vital),
                        before: change.before,
                        after: change.after,
                    }],
                )?;
            }
            SpellEffect::Transfer {
                source,
                destination,
                source_is_caster,
                destination_is_caster,
                proportion,
                loss,
                cap,
            } => {
                if !source_beneficial {
                    pk_activity::direct(world, policy, actor, target, now);
                }
                let src = if *source_is_caster { actor } else { target };
                let dst = if *destination_is_caster {
                    actor
                } else {
                    target
                };
                let a = vital_after_cost(world, src, entity_vital(*source), actor, cost)?;
                let b = vital_after_cost(world, dst, entity_vital(*destination), actor, cost)?;
                let (drain_modifier, boost_modifier) = if self.damage_profiles.contains_key(&actor)
                {
                    self.native_transfer(actor, src, dst, *source)?
                } else {
                    (1., 1.)
                };
                let transfer = if self.damage_profiles.contains_key(&actor) {
                    bace_magic::gdle_transfer
                } else {
                    |a, b, p, l, c, d: f64, e: f64| {
                        bace_magic::transfer(a, b, p, l, c, d as f32, e as f32)
                    }
                };
                let change = transfer(
                    VitalState {
                        current: a.current,
                        maximum: a.maximum,
                    },
                    VitalState {
                        current: b.current,
                        maximum: b.maximum,
                    },
                    *proportion,
                    *loss,
                    *cap,
                    drain_modifier,
                    boost_modifier,
                )
                .map_err(|_| CastRejection::InvalidState)?;
                self.commit_vitals(
                    world,
                    actor,
                    cost,
                    &[
                        VitalMutation {
                            actor: src,
                            vital: entity_vital(*source),
                            before: change.source.before,
                            after: change.source.after,
                        },
                        VitalMutation {
                            actor: dst,
                            vital: entity_vital(*destination),
                            before: change.destination.before,
                            after: change.destination.after,
                        },
                    ],
                )?;
            }
            SpellEffect::Enchantment(spec) => {
                let registry = self
                    .registries
                    .get(&target)
                    .ok_or(CastRejection::MissingAssets)?;
                let proposed = registry
                    .propose_add(
                        EnchantmentEntry {
                            spell: spell.id,
                            caster: actor.0,
                            school: spell.school,
                            spec: spec.clone(),
                            start_time: 0.0,
                            is_set_spell: spec.set_id.is_some(),
                            is_level8_aura: false,
                            metadata: self.metadata_for(spell.id, spec),
                        },
                        now,
                        false,
                    )
                    .map_err(|_| CastRejection::InvalidState)?;
                self.apply_mana(world, actor, cost)?;
                if !source_beneficial {
                    pk_activity::enchantment(
                        world,
                        policy,
                        actor,
                        observed_target.unwrap_or(target),
                        now,
                    );
                }
                let update = self
                    .registries
                    .get_mut(&target)
                    .expect("validated registry")
                    .adopt(proposed)
                    .expect("single-owner prepared mutation");
                self.events.push_back(MagicEvent::Enchantment {
                    actor: target,
                    entry: update.entry,
                });
            }
            SpellEffect::Dispel(spec) => {
                self.dispel(target, spec, &mut random)?;
                self.apply_mana(world, actor, cost)?;
            }
            SpellEffect::Projectile(spec)
            | SpellEffect::LifeProjectile {
                projectile: spec, ..
            } => {
                if self.ids.len() < usize::from(spec.count) {
                    return Ok(false);
                }
                let mut changes = Vec::new();
                let life_damage = if let SpellEffect::LifeProjectile {
                    source,
                    proportion,
                    damage_ratio,
                    ..
                } = &spell.effect
                {
                    let vital = entity_vital(*source);
                    let state = vital_after_cost(world, actor, vital, actor, cost)?;
                    let drain = bace_magic::life_projectile_drain(
                        state.current,
                        *proportion,
                        *source == Vital::Health,
                    )
                    .map_err(|_| CastRejection::InvalidState)?;
                    changes.push(VitalMutation {
                        actor,
                        vital,
                        before: state.current,
                        after: state.current - drain,
                    });
                    Some(drain as f32 * *damage_ratio)
                } else {
                    None
                };
                let mutations = self.prepare_vital_mutations(world, actor, cost, &changes)?;
                world
                    .validate_vital_batch(&mutations, Some(actor))
                    .map_err(|_| CastRejection::InvalidState)?;
                self.launch_projectiles(
                    actor,
                    attempt_target(attempt),
                    attempt.prepared.clone(),
                    projectiles::ProjectileParameters {
                        spec,
                        life_damage,
                        now,
                        initial_cast: attempt.initial_cast,
                        maximum_range: (spell.range_constant
                            + spell.range_per_skill * skill as f32)
                            .min(75.0),
                        cast_skill: skill,
                        proc_parent: self.proc_parent(attempt.origin),
                        caster_item: match attempt.origin {
                            CastOrigin::ItemProc { item, .. } => Some(item),
                            _ => None,
                        },
                    },
                    &random,
                    world,
                    observers,
                )?;
                self.commit_vitals(world, actor, cost, &changes)?;
            }
            SpellEffect::Portal(effect) | SpellEffect::FellowshipPortal(effect) => {
                self.events.push_back(MagicEvent::PortalRequired {
                    cast: attempt.cast,
                    actor,
                    target,
                    effect: effect.clone(),
                });
                attempt.portal_request_sent = true;
                return Ok(false);
            }
            SpellEffect::FellowshipBoost { .. }
            | SpellEffect::FellowshipEnchantment(_)
            | SpellEffect::FellowshipDispel(_) => {
                let members = fellowships
                    .roster(target)
                    .ok_or(CastRejection::InvalidTarget)?;
                if !self.release_fellowship(
                    attempt,
                    world,
                    policy,
                    members,
                    fellowship::FellowshipRelease {
                        now,
                        cost,
                        random: &random,
                        observers,
                    },
                )? {
                    return Ok(false);
                }
            }
        }
        attempt.random = random;
        Ok(true)
    }
    pub(super) fn apply_mana(
        &mut self,
        world: &mut World,
        actor: EntityId,
        cost: u32,
    ) -> Result<(), CastRejection> {
        self.commit_vitals(world, actor, cost, &[])
    }
    pub(super) fn commit_vitals(
        &mut self,
        world: &mut World,
        source: EntityId,
        mana_cost: u32,
        changes: &[VitalMutation],
    ) -> Result<(), CastRejection> {
        let mutations = self.prepare_vital_mutations(world, source, mana_cost, changes)?;
        let result = world
            .apply_vital_batch(&mutations, Some(source))
            .map_err(|_| CastRejection::InvalidState)?;
        for result in result {
            let m = result.mutation;
            self.events.push_back(MagicEvent::Vital {
                incarnation: world.combatant(m.actor).map_or(0, |c| c.incarnation()),
                actor: m.actor,
                vital: m.vital,
                before: m.before,
                after: m.after,
                revision: result.revision,
            });
            if m.vital == EntityVital::Health && m.after < m.before {
                let maximum = world
                    .vital(m.actor, EntityVital::Health)
                    .map_err(|_| CastRejection::MissingActor)?
                    .maximum;
                self.combat.push_back(CombatEvent::Damage {
                    target_incarnation: world.combatant(m.actor).map_or(0, |c| c.incarnation()),
                    attacker: Some(source),
                    death_blow: None,
                    target: m.actor,
                    amount: m.before - m.after,
                    current: m.after,
                    maximum,
                    killed: m.after == 0,
                    revision: result.revision,
                });
            }
        }
        Ok(())
    }
    fn prepare_vital_mutations(
        &self,
        world: &World,
        source: EntityId,
        mana_cost: u32,
        changes: &[VitalMutation],
    ) -> Result<Vec<VitalMutation>, CastRejection> {
        if self.object_casters.contains_key(&source) {
            if mana_cost != 0 {
                return Err(CastRejection::InsufficientMana);
            }
            return Ok(changes.to_vec());
        }
        let mana = world
            .vital(source, EntityVital::Mana)
            .map_err(|_| CastRejection::MissingAssets)?;
        if mana.current < mana_cost {
            return Err(CastRejection::InsufficientMana);
        }
        let mut mutations = Vec::with_capacity(changes.len() + 1);
        mutations.push(VitalMutation {
            actor: source,
            vital: EntityVital::Mana,
            before: mana.current,
            after: mana.current - mana_cost,
        });
        for change in changes {
            if let Some(existing) = mutations
                .iter_mut()
                .find(|m| m.actor == change.actor && m.vital == change.vital)
            {
                let value =
                    i64::from(existing.after) + i64::from(change.after) - i64::from(change.before);
                existing.after = u32::try_from(value).map_err(|_| CastRejection::InvalidState)?;
            } else {
                mutations.push(*change);
            }
        }
        mutations.retain(|m| m.before != m.after);
        Ok(mutations)
    }
    pub(super) fn dispel(
        &mut self,
        target: EntityId,
        spec: &DispelSpec,
        random: &mut RandomStream,
    ) -> Result<(), CastRejection> {
        let registry = self
            .registries
            .get_mut(&target)
            .ok_or(CastRejection::MissingAssets)?;
        let mut eligible = Vec::with_capacity(512);
        registry
            .dispel_candidates(spec, &mut eligible)
            .map_err(|_| CastRejection::InvalidState)?;
        for i in (1..eligible.len()).rev() {
            let j = random
                .below((i + 1) as u64)
                .map_err(|_| CastRejection::InvalidState)? as usize;
            eligible.swap(i, j);
        }
        eligible.truncate(spec.count as usize);
        registry
            .remove(&eligible)
            .map_err(|_| CastRejection::InvalidState)?;
        self.events.push_back(MagicEvent::EnchantmentsRemoved {
            actor: target,
            entries: eligible,
        });
        Ok(())
    }
}

impl Magic {
    /// ACE Creature.CalculateManaUsage: Held wand's built-in spell cost, then
    /// trained skill, spell flag and the current weapon/owner enchantment mods.
    fn mana_conversion_inputs(
        &self,
        actor: EntityId,
        spell: u32,
        base_mana: u32,
        world: &World,
    ) -> Result<(u32, u32), CastRejection> {
        let profile = self.damage_profiles.get(&actor);
        let base_mana = profile
            .and_then(|p| p.wand_spell_cost)
            .filter(|(id, _)| *id == spell)
            .map_or(base_mana, |(_, cost)| cost);
        let conversion = mana_conversion_for_spell(
            self.casters[&actor].mana_conversion,
            self.damage_spell_flags.get(&spell).copied().unwrap_or(0),
        );
        if conversion == 0 {
            return Ok((base_mana, 0));
        }
        // ACE GetWeaponManaConversionModifier casts the caster to Player;
        // nonplayer casters receive its default 1.0 modifier.
        if !self.casters[&actor].player {
            return Ok((base_mana, conversion));
        }
        let Some(weapon) = profile.and_then(|p| p.mana_conversion_weapon) else {
            return Ok((base_mana, conversion));
        };
        let item_enchantment =
            mana_conv_enchantment(self.registries.get(&EntityId(weapon.entity)))?;
        let wielder_enchantment = if weapon.enchantable {
            mana_conv_enchantment(self.registries.get(&actor))?
        } else {
            1.
        };
        let combat_mode = world.combatant(actor).is_some_and(|c| c.mode() != 1);
        let effective = bace_magic::weapon_mana_conversion_skill(
            conversion,
            weapon.base_modifier,
            item_enchantment,
            wielder_enchantment,
            combat_mode,
        )
        .map_err(|_| CastRejection::InvalidState)?;
        Ok((base_mana, effective))
    }
}

fn mana_conv_enchantment(registry: Option<&EnchantmentRegistry>) -> Result<f32, CastRejection> {
    let Some(registry) = registry else {
        return Ok(1.);
    };
    let mut modifier = 1.0f32;
    for key in [144, 171] {
        for entry in bace_magic::enchantment_modifiers(registry, 8 | 0x4000, key) {
            modifier *= entry.spec.value;
            if !modifier.is_finite() {
                return Err(CastRejection::InvalidState);
            }
        }
    }
    Ok(modifier)
}

/// ACE Creature.CalculateManaUsage checks SpellFlags.IgnoresManaConversion
/// before GetManaCost. Skill training is resolved by the character owner.
fn mana_conversion_for_spell(skill: u32, flags: u32) -> u32 {
    if flags & 0x800 != 0 { 0 } else { skill }
}

#[cfg(test)]
mod mana_conversion_tests {
    use super::{mana_conv_enchantment, mana_conversion_for_spell};
    use bace_gameplay_api::SkillAdvancement;

    #[test]
    fn ace_ignored_flag_preserves_base_cost_and_consumes_no_conversion_draws() {
        let ignored = mana_conversion_for_spell(250, 0x800);
        let ordinary = mana_conversion_for_spell(250, 0);
        assert_eq!(ignored, 0);
        assert_eq!(ordinary, 250);
        assert_eq!(
            bace_magic::mana_cost(150, 30, ignored, &[]).unwrap(),
            (30, 0)
        );
        assert!(bace_magic::mana_cost(150, 30, ordinary, &[]).is_err());
        assert_eq!(mana_conversion_for_spell(250, 0x804), 0);
    }

    #[test]
    fn ace_training_gate_uses_only_trained_and_specialized_skill() {
        for (advancement, expected) in [
            (SkillAdvancement::Inactive, 0),
            (SkillAdvancement::Untrained, 0),
            (SkillAdvancement::Trained, 250),
            (SkillAdvancement::Specialized, 250),
        ] {
            let value = bace_character::SkillValues {
                skill: 16,
                advancement,
                base: 250,
                current: 250,
            };
            assert_eq!(crate::magic::trained_mana_conversion(value), expected);
        }
    }

    #[test]
    fn ace_mana_conversion_multiplies_item_and_aura_top_layer_properties() {
        let entry = |spell, key, value| bace_magic::EnchantmentEntry {
            spell,
            caster: 1,
            school: bace_magic::MagicSchool::Creature,
            spec: bace_magic::EnchantmentSpec {
                category: key as u16,
                power: 100,
                duration: 60.,
                layer: 1,
                stat_type: 8 | 0x1000 | 0x4000,
                stat_key: key,
                value,
                beneficial: true,
                set_id: None,
            },
            start_time: 0.,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: Default::default(),
        };
        let registry = bace_magic::EnchantmentRegistry::restore(
            4,
            0,
            vec![entry(1, 144, 1.2), entry(2, 171, 1.5)],
        )
        .unwrap();
        assert_eq!(
            mana_conv_enchantment(Some(&registry)).unwrap(),
            1.2f32 * 1.5f32
        );
        assert_eq!(mana_conv_enchantment(None).unwrap(), 1.);
    }

    #[test]
    fn live_wand_cost_and_enchanted_weapon_modifier_feed_cast_release() {
        use crate::magic::{Magic, MagicCaster};
        use bace_types::EntityId;
        use std::collections::BTreeSet;
        let mut magic = Magic::new(16);
        let actor = EntityId(1);
        magic.casters.insert(
            actor,
            MagicCaster {
                player: true,
                known_spells: BTreeSet::new(),
                school_skills: [100; 5],
                magic_defense: 0,
                mana_conversion: 100,
                components_required: false,
                safe_components: true,
            },
        );
        let mut profile = bace_magic::MagicDamageProfile::neutral(true);
        profile.mana_conversion_weapon = Some(bace_magic::ManaConversionWeapon {
            entity: 2,
            base_modifier: 0.1,
            enchantable: true,
        });
        profile.wand_spell_cost = Some((123, 7));
        magic.damage_profiles.insert(actor, profile);
        let entry = |spell, key, value| bace_magic::EnchantmentEntry {
            spell,
            caster: 1,
            school: bace_magic::MagicSchool::Creature,
            spec: bace_magic::EnchantmentSpec {
                category: key as u16,
                power: 100,
                duration: 60.,
                layer: 1,
                stat_type: 8 | 0x1000 | 0x4000,
                stat_key: key,
                value,
                beneficial: true,
                set_id: None,
            },
            start_time: 0.,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: Default::default(),
        };
        magic.registries.insert(
            EntityId(2),
            bace_magic::EnchantmentRegistry::restore(4, 0, vec![entry(1, 144, 1.2)]).unwrap(),
        );
        magic.registries.insert(
            actor,
            bace_magic::EnchantmentRegistry::restore(4, 0, vec![entry(2, 171, 1.5)]).unwrap(),
        );
        let world = crate::magic::specialization_tests::world();
        assert_eq!(
            magic.mana_conversion_inputs(actor, 123, 30, &world),
            Ok((7, 118))
        );
        assert_eq!(
            magic.mana_conversion_inputs(actor, 999, 30, &world),
            Ok((30, 118))
        );
        magic.damage_spell_flags.insert(123, 0x800);
        assert_eq!(
            magic.mana_conversion_inputs(actor, 123, 30, &world),
            Ok((7, 0))
        );
    }
}
