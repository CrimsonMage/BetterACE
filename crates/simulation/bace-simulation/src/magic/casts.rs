use super::*;
impl Magic {
    // Separate owner borrows keep launch audiences exact without a binding cache.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply(
        &mut self,
        context: ActionContext,
        request: CastRequest,
        world: &mut World,
        now: f64,
        policy: &Combat,
        fellowships: &crate::fellowships::Fellowships,
        observers: Option<(&crate::characters::Characters, u64)>,
    ) -> Result<CastChange, CastRejection> {
        self.apply_origin(
            CastOrigin::Player(context),
            request,
            world,
            now,
            policy,
            fellowships,
            observers,
        )
    }
    // Separate owner borrows keep launch audiences exact without a binding cache.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_origin(
        &mut self,
        origin: CastOrigin,
        request: CastRequest,
        world: &mut World,
        now: f64,
        policy: &Combat,
        fellowships: &crate::fellowships::Fellowships,
        observers: Option<(&crate::characters::Characters, u64)>,
    ) -> Result<CastChange, CastRejection> {
        if let Some((_, tick)) = observers {
            self.event_tick = tick;
        }
        let actor = origin.actor();
        if let Some((kind, event)) = origin.server_event() {
            let queued = self.item_proc_request(origin);
            if event == 0
                || queued.is_some_and(|p| p.admitted)
                || queued.is_none()
                    && self
                        .server_sequences
                        .get(&(actor, kind))
                        .is_some_and(|last| event <= *last)
            {
                return Err(CastRejection::StaleSequence);
            }
        }
        if !self.can_accept() {
            return Err(CastRejection::Capacity);
        }
        if request == CastRequest::Cancel {
            if self.attempts.get(&actor).is_some_and(|a| {
                (a.component_request_sent && !a.components_confirmed)
                    || a.portal_operation.is_some()
                    || a.terminal.is_some()
                    || a.pending_direct.is_some()
            }) {
                return Err(CastRejection::Busy);
            }
            let cast = self
                .attempts
                .get(&actor)
                .ok_or(CastRejection::InvalidState)?
                .cast;
            self.admit_cast_stop(actor, cast, world)?;
            let mut attempt = self
                .attempts
                .remove(&actor)
                .ok_or(CastRejection::InvalidState)?;
            attempt.driver.cancel();
            restore_monster_mode(&attempt, world);
            self.retain_recovery(&attempt, None, now)?;
            world
                .body_mut(actor)
                .map_err(|_| CastRejection::MissingActor)?
                .stop_motion();
            return Ok(CastChange::Cancelled { cast: attempt.cast });
        }
        let (spell, mut target) = match request {
            CastRequest::Targeted { target, spell } => (spell, Some(target)),
            CastRequest::Untargeted { spell } => (spell, None),
            CastRequest::Cancel => unreachable!(),
        };
        let prepared = if origin.instant() {
            self.spells
                .get(&spell)
                .cloned()
                .ok_or(CastRejection::UnknownSpell)?
        } else {
            self.prepared_for_actor(actor, spell)?
        };
        if self
            .damage_spell_flags
            .get(&spell)
            .is_some_and(|flags| flags & 8 != 0)
        {
            target = Some(actor);
        }
        let item_target = self.item_target_for(actor, target, &prepared.spell, world)?;
        let observed_target = item_target.map(|i| i.owner).or(target);
        self.validate_damage_preparation(actor, target, &prepared.spell, world)?;
        let independent_instant = origin.instant();
        if independent_instant && self.instant_continuations.len() >= self.capacity {
            return Err(CastRejection::Capacity);
        }
        let style_chain = if matches!(origin, CastOrigin::Player(_))
            && policy.has_physical_driver(actor)
            && world
                .source_motion_state(actor)
                .is_some_and(|s| s.style != 0x80000049 || s.substate != 0x41000003)
        {
            Some(
                policy
                    .physical_style_chain(world, actor, 0x80000049)
                    .ok_or(CastRejection::MissingAssets)?,
            )
        } else {
            None
        };
        if self.attempts.contains_key(&actor) && !independent_instant {
            return Err(CastRejection::Busy);
        }
        if !origin.instant()
            && world
                .body(actor)
                .is_ok_and(|b| b.collision_shape().is_some())
            && world.source_motion_state(actor).is_none()
        {
            return Err(CastRejection::MissingAssets);
        }
        if !origin.instant() && prepared.gestures.is_empty() {
            return Err(CastRejection::MissingAssets);
        }
        if !origin.instant()
            && prepared.gestures.iter().any(|g| g.motion_chain.is_none())
            && (!self.synthetic_cast_timing
                || world
                    .body(actor)
                    .is_ok_and(|body| body.collision_shape().is_some()))
        {
            return Err(CastRejection::MissingAssets);
        }
        if matches!(origin, CastOrigin::Player(_))
            && prepared
                .gestures
                .iter()
                .filter_map(|g| g.motion_chain.as_ref())
                .any(|chain| !chain.is_rootless())
        {
            return Err(CastRejection::MissingAssets);
        }
        if !origin.instant()
            && prepared
                .gestures
                .iter()
                .filter_map(|g| g.motion_chain.as_ref())
                .any(|c| c.stop_chain().is_none())
        {
            return Err(CastRejection::MissingAssets);
        }
        // Typed coverage is not execution coverage: never silently turn an
        // unported life drain, arc/strike trajectory or fellowship fanout into
        // an ordinary projectile/single-target spell.
        match &prepared.spell.effect {
            SpellEffect::Projectile(spec)
            | SpellEffect::LifeProjectile {
                projectile: spec, ..
            } if (spec.shape == bace_magic::ProjectileShape::Strike
                && (spec.gravity != 0.0 || target.is_none_or(|id| id == actor)))
                || (spec.gravity != 0.0 && spec.gravity != 9.8) =>
            {
                return Err(CastRejection::MissingAssets);
            }
            _ => {}
        }
        if !matches!(
            prepared.spell.effect,
            SpellEffect::Portal(bace_magic::PortalEffect::Link { .. })
        ) && (!matches!(origin, CastOrigin::Staff { .. })
            || matches!(
                prepared.spell.effect,
                SpellEffect::Dispel(_) | SpellEffect::FellowshipDispel(_)
            ))
        {
            self.spell_permission(
                policy,
                actor,
                observed_target.unwrap_or(actor),
                helpful_spell(&prepared.spell),
                world,
            )?;
        }
        let has_enchantment = match &prepared.spell.effect {
            SpellEffect::Enchantment(_) | SpellEffect::FellowshipEnchantment(_) => true,
            SpellEffect::Projectile(p) | SpellEffect::LifeProjectile { projectile: p, .. } => {
                p.enchantment.is_some()
            }
            _ => false,
        };
        if has_enchantment && !self.enchantment_metadata.contains_key(&spell) {
            return Err(CastRejection::MissingAssets);
        }
        if self.registry_clocks.get(&actor).is_some_and(|c| c.reserved)
            || target
                .and_then(|id| self.registry_clocks.get(&id))
                .is_some_and(|c| c.reserved)
        {
            return Err(CastRejection::Busy);
        }
        let required = match &prepared.spell.effect {
            SpellEffect::Projectile(p) => {
                (usize::from(p.count) + 1).max(2 + usize::from(p.enchantment.is_some()))
            }
            SpellEffect::LifeProjectile { projectile: p, .. } => usize::from(p.count) + 2,
            SpellEffect::Transfer { .. } => 3,
            _ => 2,
        };
        if required > self.capacity {
            return Err(CastRejection::Capacity);
        }
        let caster = self
            .casters
            .get(&actor)
            .ok_or(CastRejection::MissingAssets)?;
        if matches!(origin, CastOrigin::Player(_)) && !caster.known_spells.contains(&spell) {
            return Err(CastRejection::UnlearnedSpell);
        }
        let skill = self.effective_cast_skill(origin, &prepared.spell, world)?;
        if skill == 0 && matches!(origin, CastOrigin::Player(_)) {
            return Err(CastRejection::UntrainedSchool);
        }
        let object = self.object_casters.contains_key(&actor);
        if object && !origin.instant() {
            return Err(CastRejection::MissingAssets);
        }
        if object
            && matches!(
                &prepared.spell.effect,
                SpellEffect::LifeProjectile { .. }
                    | SpellEffect::Transfer {
                        source_is_caster: true,
                        ..
                    }
                    | SpellEffect::Transfer {
                        destination_is_caster: true,
                        ..
                    }
            )
        {
            return Err(CastRejection::MissingAssets);
        }
        let mut observation = observe_kind(
            world,
            actor,
            observed_target,
            &prepared.spell,
            skill,
            object,
        )?;
        if !matches!(origin, CastOrigin::Player(_)) {
            observation.peace_mode = false;
        }
        if matches!(origin, CastOrigin::Staff { .. }) {
            observation.target_in_range = true;
            observation.geometry_clear = true;
        }
        if origin.instant() {
            observation.heading_to_target = None;
            observation.turning_to_target = false;
            observation.manual_turning = false;
        }
        let mana_current = if object {
            0
        } else {
            world
                .vital(actor, EntityVital::Mana)
                .map_err(|_| CastRejection::MissingAssets)?
                .current
        };
        let creature_uses_mana = server_casting::creature_origin(origin)
            && self.server_mana.get(&actor).copied().unwrap_or(true);
        if creature_uses_mana && mana_current < prepared.spell.base_mana {
            return Err(CastRejection::InsufficientMana);
        }
        if !server_casting::creature_origin(origin)
            && origin.uses_resources()
            && mana_current < prepared.spell.base_mana
        {
            return Err(CastRejection::InsufficientMana);
        }
        let next = self
            .next_cast
            .checked_add(1)
            .ok_or(CastRejection::InvalidState)?;
        if server_casting::creature_origin(origin)
            && self.spell_categories.get(&spell) == Some(&67)
            && world
                .combatant(actor)
                .is_some_and(|c| c.health() == c.profile().maximum_health)
        {
            self.admit_server_origin(origin);
            self.next_cast = next;
            self.publish_outcome(origin, Ok(CastChange::Completed { cast: next }));
            return Ok(CastChange::Started { cast: next });
        }
        let creature_mana = self.prepare_creature_mana(origin, prepared.spell.base_mana, world)?;
        let identity = origin_identity(origin);
        let random = self
            .random
            .as_ref()
            .ok_or(CastRejection::MissingAssets)?
            .event_stream(identity, Domain::Magic)
            .and_then(|stream| stream.fork(b"execution_epoch", self.execution_epoch))
            .and_then(|stream| {
                stream.fork(
                    if matches!(origin, CastOrigin::Player(_)) {
                        b"player_cast"
                    } else {
                        b"server_cast"
                    },
                    0,
                )
            })
            .map_err(|_| CastRejection::InvalidState)?;
        let recovery = self.recovery.get(&actor).copied().unwrap_or_default();
        if !recovery.can_mutate() {
            return Err(CastRejection::InvalidState);
        }
        let mut driver = CastDriver::default();
        let (minimum, streak) = recovery.deadlines();
        let minimum = if matches!(origin, CastOrigin::Player(_)) {
            minimum
        } else {
            0.
        };
        if !origin.instant() {
            driver
                .restore_recovery_deadlines(minimum, streak)
                .map_err(rejection)?;
        }
        let cast_preparation = CastPreparation {
            id: next,
            spell,
            target: observed_target.map(|id| id.0),
            gestures: if origin.instant() {
                vec![]
            } else {
                prepared.gestures.iter().map(|g| g.gesture).collect()
            },
            uses_mana: origin.uses_resources(),
            player: matches!(origin, CastOrigin::Player(_)),
            fast_resistable_pk_spell: origin.uses_resources() && prepared.fast_resistable_pk_spell,
        };
        let signal = driver
            .begin(cast_preparation.clone(), now, observation)
            .map_err(rejection)?;
        if style_chain.is_some() {
            driver = CastDriver::default();
            driver
                .restore_recovery_deadlines(minimum, streak)
                .map_err(rejection)?;
        }
        let mut attempt = Attempt {
            item_target,
            origin,
            initial_cast: {
                let (cell, state) = world
                    .actor_state(actor)
                    .map_err(|_| CastRejection::MissingActor)?;
                let body = world.body(actor).map_err(|_| CastRejection::MissingActor)?;
                (
                    cell,
                    state.position()
                        - if body.collision_shape().is_none() {
                            Vec3::new(0.0, 0.0, body.collision_radius())
                        } else {
                            Vec3::ZERO
                        },
                )
            },
            origin_epoch: world
                .body(actor)
                .map_err(|_| CastRejection::MissingActor)?
                .accepted()
                .epoch(),
            target,
            prepared,
            driver,
            random,
            resources: None,
            pending_direct: None,
            cast_skill: skill,
            motion: None,
            style_entry: None,
            motion_sequence_offset: 0,
            turn_target: None,
            turn_control: None,
            cast: next,
            components_confirmed: !matches!(origin, CastOrigin::Player(_))
                || !caster.components_required,
            component_request_sent: false,
            component_failure: None,
            mana_applied: false,
            portal_request_sent: false,
            portal_operation: None,
            portal_completion: None,
            terminal: None,
            peace_fizzle: None,
            previous_mode: None,
        };
        if independent_instant {
            if let CastSignal::Finished {
                error: Some(error), ..
            } = signal
            {
                return Err(rejection(error));
            }
            if !matches!(signal, CastSignal::Release { .. }) {
                return Err(CastRejection::InvalidState);
            }
            if !self.release(&mut attempt, world, now, policy, fellowships, observers)? {
                if attempt.pending_direct.is_some() || attempt.portal_request_sent {
                    self.admit_server_origin(origin);
                    self.next_cast = next;
                    self.instant_continuations.insert(next, attempt);
                    return Ok(CastChange::Started { cast: next });
                }
                return Err(CastRejection::Capacity);
            }
            self.admit_server_origin(origin);
            self.next_cast = next;
            self.publish_outcome(origin, Ok(CastChange::Completed { cast: next }));
            return Ok(CastChange::Started { cast: next });
        }
        if attempt.previous_mode.is_some()
            && let Some(state) = world.combatant_mut(actor)
        {
            state.set_mode(8);
        }
        if let Some(chain) = style_chain {
            self.begin_style_entry(&mut attempt, chain, cast_preparation, world, now)?;
        } else {
            self.signal(&mut attempt, signal, world, now)?;
        }
        if let Some(change) = creature_mana {
            world
                .apply_vital_batch(&[change], None)
                .expect("same-owner creature mana preflight");
            self.events.push_back(MagicEvent::Vital {
                incarnation: world.combatant(actor).map_or(0, |c| c.incarnation()),
                actor,
                vital: change.vital,
                before: change.before,
                after: change.after,
                revision: world.combatant(actor).expect("checked caster").revision(),
            });
        }
        self.admit_server_origin(origin);
        self.next_cast = next;
        self.attempts.insert(actor, attempt);
        Ok(CastChange::Started { cast: next })
    }
}
