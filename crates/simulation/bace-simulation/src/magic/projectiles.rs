use super::*;
pub(super) struct ProjectileParameters<'a> {
    pub spec: &'a ProjectileSpec,
    pub life_damage: Option<f32>,
    pub now: f64,
    pub initial_cast: (CellId, Vec3),
    pub maximum_range: f32,
    pub cast_skill: u32,
    pub caster_item: Option<EntityId>,
    pub proc_parent: Option<(CastOrigin, u8)>,
}

impl Magic {
    // Separate owner borrows keep launch audiences exact without a binding cache.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn launch_projectiles(
        &mut self,
        actor: EntityId,
        target: Option<EntityId>,
        spell: Arc<PreparedMagicSpell>,
        parameters: ProjectileParameters<'_>,
        random: &RandomStream,
        world: &mut World,
        observers: Option<(&crate::characters::Characters, u64)>,
    ) -> Result<(), CastRejection> {
        let spec = parameters.spec;
        if spec.count == 0 || self.flying.len() + usize::from(spec.count) > self.capacity {
            return Err(CastRejection::Capacity);
        }
        let (cell, trajectories) =
            super::projectile_launch::trajectories(actor, target, spec, random, world)?;
        if world
            .actor_state(actor)
            .map_err(|_| CastRejection::MissingActor)?
            .0
            != parameters.initial_cast.0
        {
            return Err(CastRejection::MissingAssets);
        }
        let lifetime = bace_magic::SpellProjectileLifetime::new(parameters.now)
            .map_err(|_| CastRejection::InvalidState)?;
        let collision_radius = if let Some(shape) = self.projectile_shapes.get(&spec.template) {
            if shape.nominal_radius() != Some(spec.radius) {
                return Err(CastRejection::MissingAssets);
            }
            shape.spheres()[0].radius
        } else {
            if world
                .body(actor)
                .is_ok_and(|body| body.collision_shape().is_some())
            {
                return Err(CastRejection::MissingAssets);
            }
            spec.radius // explicit synthetic fixture only
        };
        let cast_skill = if let Some(item) = parameters.caster_item.and_then(|item| {
            self.damage_profiles
                .get(&actor)
                .and_then(|p| p.proc_items.iter().find(|p| p.item == item.0))
        }) {
            bace_magic::magic_cloak_projectile_skill(item, spell.spell.id, parameters.cast_skill)
                .map_err(|_| CastRejection::InvalidState)?
        } else {
            parameters.cast_skill
        };
        let mut prepared = Vec::with_capacity(trajectories.len());
        for (index, launch) in trajectories.into_iter().enumerate() {
            let id = *self.ids.get(index).ok_or(CastRejection::Capacity)?;
            if world.contains_identity(id) {
                return Err(CastRejection::InvalidState);
            }
            let position = launch.position;
            let shot_velocity = launch.velocity;
            let body = ProjectileBody::new(
                position,
                shot_velocity,
                collision_radius,
                -spec.gravity,
                30.0,
            )
            .map_err(|_| CastRejection::InvalidState)?;
            let stream = random
                .fork(b"projectile", index as u64)
                .map_err(|_| CastRejection::InvalidState)?;
            prepared.push((
                id,
                OwnedProjectile {
                    cell,
                    source: actor,
                    target: if spec.count == 1 { target } else { None },
                    body,
                },
                stream,
            ));
        }
        let mut inserted = Vec::with_capacity(prepared.len());
        let mut announcements = Vec::with_capacity(prepared.len());
        for (id, projectile, stream) in prepared {
            if let Err((_error, _projectile)) = world.insert_projectile(id, projectile) {
                for id in inserted {
                    world.remove_projectile(id);
                    self.flying.remove(&id);
                }
                return Err(CastRejection::MissingAssets);
            }
            inserted.push(id);
            let launch = if let Some((characters, tick)) = observers {
                match world
                    .projectile_launch_snapshot(id, tick, |actor| characters.entered_binding(actor))
                {
                    Ok(snapshot) => Some(Arc::new(snapshot)),
                    Err(_) => {
                        for id in inserted {
                            world.remove_projectile(id);
                            self.flying.remove(&id);
                        }
                        return Err(CastRejection::MissingAssets);
                    }
                }
            } else {
                None
            };
            self.flying.insert(
                id,
                Flying {
                    proc_parent: parameters.proc_parent,
                    pending_damage: None,
                    launch_wand: self
                        .damage_profiles
                        .get(&actor)
                        .and_then(|p| p.wand.clone()),
                    source: actor,
                    target: if spec.count == 1 { target } else { None },
                    spell: spell.clone(),
                    random: stream,
                    lifetime,
                    initial_cast: parameters.initial_cast,
                    maximum_range: parameters.maximum_range,
                    cast_skill,
                    resting: false,
                    pending_impact: None,
                    life_damage: parameters.life_damage,
                    credited_owner: world.damage_owner(actor),
                },
            );
            let accepted = world.projectile(id).expect("inserted projectile");
            announcements.push(MagicEvent::ProjectileCreated {
                actor: id,
                launch,
                effect_intensity: bace_magic::spell_projectile_intensity(
                    self.damage_spell_levels
                        .get(&spell.spell.id)
                        .copied()
                        .unwrap_or(0),
                ),
                source: actor,
                spell: spell.spell.id,
                template: spec.template,
                position: accepted.body.position(),
                velocity: accepted.body.velocity(),
            });
        }
        self.events.extend(announcements);
        self.ids.drain(..inserted.len());
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn step_projectiles(&mut self, world: &mut World, now: f64, policy: &Combat) {
        self.step_projectiles_with_observers(world, now, policy, None);
    }
    pub(super) fn step_projectiles_with_observers(
        &mut self,
        world: &mut World,
        now: f64,
        policy: &Combat,
        observers: Option<(&crate::characters::Characters, u64)>,
    ) {
        let mut ids = std::mem::take(&mut self.projectile_scratch);
        ids.clear();
        ids.extend(self.flying.keys().copied());
        for id in ids.iter().copied() {
            let needed = self
                .flying
                .get(&id)
                .map_or(1, |f| match &f.spell.spell.effect {
                    SpellEffect::Projectile(p)
                    | SpellEffect::LifeProjectile { projectile: p, .. } => {
                        2 + usize::from(p.enchantment.is_some())
                    }
                    _ => 2,
                });
            if self.events.len() + needed > self.capacity || self.combat.len() + 1 > self.capacity {
                break;
            }
            let Some(mut flying) = self.flying.remove(&id) else {
                continue;
            };
            if flying.pending_damage.is_some() {
                self.commit_projectile_damage(&mut flying, world, now);
                self.flying.insert(id, flying);
                continue;
            }
            if !self.proc_capacity() {
                self.flying.insert(id, flying);
                break;
            }
            if flying.lifetime.exploded() || flying.resting || now <= flying.lifetime.spawned_at() {
                self.flying.insert(id, flying);
                continue;
            }
            let step = if let Some(target) = flying.pending_impact.take() {
                Ok(ProjectileStep::Impact { target })
            } else {
                world.step_projectile_filtered(id, 1.0 / 30.0, |world, candidate| {
                    // ObjCollision still blocks on physical non-combat objects.
                    let Some(_) = world.combatant(candidate) else {
                        return true;
                    };
                    candidate != flying.source
                        && flying.target.is_none_or(|required| required == candidate)
                })
            };
            match step {
                Ok(ProjectileStep::Impact { target }) => {
                    if target
                        .and_then(|id| self.registry_clocks.get(&EntityId(id)))
                        .is_some_and(|clock| {
                            clock.reserved
                                || clock.error.is_some()
                                || clock.active && clock.next_due <= now
                        })
                    {
                        flying.pending_impact = Some(target);
                        self.flying.insert(id, flying);
                        continue;
                    }
                    // Damage immunity is not a physics pass-through rule. The
                    // current one-impact body conservatively holds blocked
                    // contacts; post-impact gravity/normal response is not yet
                    // source-qualified and must not be described as parity.
                    flying.resting = true;
                    let damage_target = target
                        .map(EntityId)
                        .filter(|target| world.combatant(*target).is_some());
                    if let Some(target) = damage_target
                        && (world.actor_state(flying.source).is_err()
                            || world.damage_owner(target) != target
                            || world.combatant(target).is_some_and(|s| s.health() == 0)
                            || flying.credited_owner != flying.source
                                && world.combatant(target).is_some_and(|s| s.profile().player)
                            || self
                                .spell_permission(
                                    policy,
                                    flying.credited_owner,
                                    target,
                                    false,
                                    world,
                                )
                                .is_err())
                    {
                        self.flying.insert(id, flying);
                        continue;
                    }
                    if matches!(
                        flying.lifetime.collide(now, damage_target.is_some()),
                        Ok(bace_magic::ProjectileLifeAction::Explode)
                    ) {
                        self.events.push_back(MagicEvent::ProjectileExploded {
                            tick: self.event_tick,
                            actor: id,
                            spell: flying.spell.spell.id,
                        });
                    }
                    if let Some(target) = target.map(EntityId)
                        && world.combatant(target).is_some_and(|s| s.health() != 0)
                    {
                        // SpellProjectile.cpp119: accepted player contact tags
                        // PK activity before the damage/resistance branch.
                        crate::pk_activity::record_pair(world, flying.source, target, now);
                        if flying.spell.spell.resistable {
                            let skill = flying.cast_skill;
                            let defense = self.casters.get(&target).map_or(0, |c| c.magic_defense);
                            if unit(&mut flying.random)
                                .and_then(|draw| {
                                    bace_magic::resisted(skill, defense, draw)
                                        .map(|v| v.0)
                                        .map_err(|_| CastRejection::InvalidState)
                                })
                                .unwrap_or(true)
                                || world
                                    .combatant(target)
                                    .is_some_and(|c| c.lifestone_protected())
                            {
                                match self.sigil_procs(flying.source, None, &mut flying.random) {
                                    Ok(procs) => {
                                        for proc in procs {
                                            self.queue_item_proc(
                                                flying.source,
                                                proc,
                                                flying.proc_parent,
                                            );
                                        }
                                    }
                                    Err(reason) => self.target_rejected(
                                        flying.source,
                                        target,
                                        flying.spell.spell.id,
                                        reason,
                                        world,
                                        observers,
                                    ),
                                }
                                self.flying.insert(id, flying);
                                continue;
                            }
                        }
                        let spec = match &flying.spell.spell.effect {
                            SpellEffect::Projectile(p)
                            | SpellEffect::LifeProjectile { projectile: p, .. } => Some(p),
                            _ => None,
                        };
                        if let Some(spec) = spec {
                            let vital_kind = match spec.damage_type {
                                0x100 => EntityVital::Stamina,
                                0x200 => EntityVital::Mana,
                                _ => EntityVital::Health,
                            };
                            let mut impact_random = flying.random.clone();
                            let damage = if spec.enchantment.is_some() {
                                Ok(0)
                            } else {
                                self.resolved_projectile_damage(
                                    &flying,
                                    target,
                                    id,
                                    spec,
                                    &mut impact_random,
                                    world,
                                )
                            };
                            flying.random = impact_random;
                            if let Err(reason) = damage {
                                self.target_rejected(
                                    flying.source,
                                    target,
                                    flying.spell.spell.id,
                                    reason,
                                    world,
                                    observers,
                                );
                            }
                            if let Ok(damage) = damage {
                                let cloak = if spec.enchantment.is_some() {
                                    Ok(bace_magic::MagicCloakResult {
                                        damage: 0,
                                        cast: None,
                                    })
                                } else {
                                    self.cloak_proc(
                                        flying.source,
                                        target,
                                        damage,
                                        world,
                                        &mut flying.random,
                                    )
                                };
                                let sigils = self.sigil_procs(
                                    flying.source,
                                    Some(target),
                                    &mut flying.random,
                                );
                                match (cloak, sigils) {
                                    (Ok(cloak), Ok(sigils)) => {
                                        let cloak_wait = cloak.cast.and_then(|proc| {
                                            self.queue_item_proc(target, proc, flying.proc_parent)
                                        });
                                        flying.pending_damage = Some(PendingProjectileDamage {
                                            apply_damage: spec.enchantment.is_none(),
                                            target,
                                            damage: cloak.damage,
                                            vital: vital_kind,
                                            cloak_wait,
                                            sigils,
                                        });
                                        self.commit_projectile_damage(&mut flying, world, now);
                                    }
                                    (Err(reason), _) | (_, Err(reason)) => self.target_rejected(
                                        flying.source,
                                        target,
                                        flying.spell.spell.id,
                                        reason,
                                        world,
                                        observers,
                                    ),
                                }
                            }
                        }
                    }
                    self.flying.insert(id, flying);
                }
                Ok(ProjectileStep::Expired | ProjectileStep::Finished) => {
                    world.remove_projectile(id);
                    self.events.push_back(MagicEvent::ProjectileRemoved {
                        actor: id,
                        tick: self.event_tick,
                    });
                }
                Ok(ProjectileStep::Flying) => {
                    self.flying.insert(id, flying);
                }
                Err(_) => {
                    world.remove_projectile(id);
                    self.events.push_back(MagicEvent::ProjectileRemoved {
                        actor: id,
                        tick: self.event_tick,
                    });
                }
            }
        }
        // GDLE WorldLandBlock advances physics/collision before projectile Tick.
        // Check distance after this quantum, including at range boundaries.
        for id in ids.iter().copied() {
            if self.events.len() >= self.capacity {
                break;
            }
            let Some(flying) = self.flying.get_mut(&id) else {
                continue;
            };
            if flying.pending_impact.is_some() || flying.pending_damage.is_some() {
                continue;
            }
            let distance =
                world.projectile_distance_from(id, flying.initial_cast.0, flying.initial_cast.1);
            let action = flying.lifetime.tick(
                now,
                distance.is_ok(),
                distance.unwrap_or(0.0),
                flying.maximum_range,
            );
            match action {
                Ok(bace_magic::ProjectileLifeAction::Destroy) | Err(_) => {
                    world.remove_projectile(id);
                    self.flying.remove(&id);
                    self.events.push_back(MagicEvent::ProjectileRemoved {
                        actor: id,
                        tick: self.event_tick,
                    });
                }
                Ok(bace_magic::ProjectileLifeAction::Explode) => {
                    self.events.push_back(MagicEvent::ProjectileExploded {
                        tick: self.event_tick,
                        actor: id,
                        spell: flying.spell.spell.id,
                    });
                }
                Ok(bace_magic::ProjectileLifeAction::None) => {}
            }
        }
        ids.clear();
        self.projectile_scratch = ids;
    }
}
