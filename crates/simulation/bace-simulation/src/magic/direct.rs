//! Direct GDLE vital effects and source-ordered cloak continuations.
use super::*;
pub(super) struct PendingDirectDamage {
    pub target: EntityId,
    pub vital: EntityVital,
    pub damage: u32,
    pub cloak_wait: Option<u64>,
    pub random: RandomStream,
}
impl Magic {
    pub(super) fn release_direct_boost(
        &mut self,
        attempt: &mut Attempt,
        world: &mut World,
        cost: u32,
        random: &mut RandomStream,
    ) -> Result<bool, CastRejection> {
        let actor = attempt.origin.actor();
        let target = attempt.target.unwrap_or(actor);
        if let Some(pending) = attempt.pending_direct.as_ref() {
            if self.registry_reserved(pending.target)
                || world.vital_reserved(pending.target, pending.vital)
            {
                return Ok(false);
            }
            if pending
                .cloak_wait
                .is_some_and(|event| !self.completed_item_procs.contains(&event))
            {
                return Ok(false);
            }
            let current = vital_after_cost(world, pending.target, pending.vital, actor, cost)?;
            self.commit_vitals(
                world,
                actor,
                cost,
                &[VitalMutation {
                    actor: pending.target,
                    vital: pending.vital,
                    before: current.current,
                    after: current.current.saturating_sub(pending.damage),
                }],
            )?;
            let pending = attempt
                .pending_direct
                .take()
                .expect("checked direct continuation");
            if let Some(event) = pending.cloak_wait {
                self.completed_item_procs.remove(&event);
            }
            attempt.random = pending.random;
            return Ok(true);
        }
        let SpellEffect::Boost {
            vital,
            minimum,
            maximum,
        } = attempt.prepared.spell.effect
        else {
            return Err(CastRejection::InvalidState);
        };
        let dtype = match vital {
            Vital::Health => 128,
            Vital::Stamina => 256,
            Vital::Mana => 512,
        };
        if minimum < 0 || maximum < 0 {
            let mut source = self.live_damage_profile(actor)?;
            self.compose_damage_wand(actor, &mut source, false)?;
            if matches!(attempt.origin, CastOrigin::ItemProc { .. }) {
                source.wand = None;
            }
            let defender = self.live_damage_profile(target)?;
            if minimum > 0 || maximum > 0 {
                return Err(CastRejection::InvalidState);
            }
            if !self.proc_capacity() && self.proc_parent(attempt.origin).is_none() {
                return Ok(false);
            }
            let lo = minimum.unsigned_abs().min(maximum.unsigned_abs());
            let hi = minimum.unsigned_abs().max(maximum.unsigned_abs());
            let input = bace_magic::MagicDamageInput {
                source: &source,
                target: &defender,
                school: attempt.prepared.spell.school,
                skill: attempt.cast_skill,
                formula_level: *self
                    .damage_spell_levels
                    .get(&attempt.prepared.spell.id)
                    .ok_or(CastRejection::MissingAssets)?,
                damage_type: dtype,
                minimum: lo,
                maximum: hi,
                life_damage: None,
                projectile: false,
                target_in_combat: !accepted_peace_style(world, target),
                target_angle_degrees: 0.,
                rolls: bace_magic::MagicDamageRolls {
                    variance: f64::from(unit(random)?),
                    critical: f64::from(unit(random)?),
                    critical_defense: 0.,
                    sneak: 0.,
                },
            };
            let before = bace_magic::magic_damage_before_mitigation(&input)
                .map_err(|_| CastRejection::InvalidState)?;
            let rating = bace_combat::physical::physical_rating_modifier(before.rating)
                .map_err(|_| CastRejection::InvalidState)?;
            let damage = bace_magic::magic_damage_after_mitigation(&input, before, rating)
                .map_err(|_| CastRejection::InvalidState)?;
            let cloak = self.cloak_proc(actor, target, damage, world, random)?;
            // LaunchSpellEffect charges source mana before TakeDamage enters
            // the synchronous cloak callback. Freeze that ordering across waits.
            self.apply_mana(world, actor, cost)?;
            attempt.mana_applied = true;
            let cloak_wait = cloak.cast.and_then(|proc| {
                self.queue_item_proc(target, proc, self.proc_parent(attempt.origin))
            });
            attempt.pending_direct = Some(PendingDirectDamage {
                target,
                vital: entity_vital(vital),
                damage: cloak.damage,
                cloak_wait,
                random: random.clone(),
            });
            return self.release_direct_boost(attempt, world, 0, random);
        }
        let current = vital_after_cost(world, target, entity_vital(vital), actor, cost)?;
        let change = self.native_boost_heal(
            attempt.origin,
            target,
            vital,
            (minimum, maximum),
            random,
            VitalState {
                current: current.current,
                maximum: current.maximum,
            },
        )?;
        self.commit_vitals(
            world,
            actor,
            cost,
            &[VitalMutation {
                actor: target,
                vital: entity_vital(vital),
                before: change.before,
                after: change.after,
            }],
        )?;
        attempt.random = random.clone();
        Ok(true)
    }
    pub(super) fn native_boost_heal(
        &self,
        origin: CastOrigin,
        target: EntityId,
        vital: Vital,
        range: (i32, i32),
        random: &mut RandomStream,
        current: VitalState,
    ) -> Result<bace_magic::VitalChange, CastRejection> {
        let actor = origin.actor();
        let (minimum, maximum) = range;
        if minimum < 0 || maximum < 0 {
            return Err(CastRejection::InvalidState);
        }
        let mut source = self.live_damage_profile(actor)?;
        self.compose_damage_wand(actor, &mut source, false)?;
        if matches!(origin, CastOrigin::ItemProc { .. }) {
            source.wand = None;
        }
        let defender = self.live_damage_profile(target)?;
        let dtype = match vital {
            Vital::Health => 128,
            Vital::Stamina => 256,
            Vital::Mana => 512,
        };
        let raw = f64::from(minimum.min(maximum))
            + f64::from(minimum.abs_diff(maximum)) * f64::from(unit(random)?);
        let mut elemental = source
            .wand
            .as_ref()
            .filter(|w| w.damage_type == dtype)
            .map_or(1., |w| w.elemental_modifier);
        if source.player && defender.player {
            elemental = ((elemental - 1.) / 2.) + 1.;
        }
        let rating = defender.healing_ratings[0]
            .checked_sub(defender.healing_ratings[1])
            .ok_or(CastRejection::InvalidState)?;
        let modifier = bace_combat::physical::physical_rating_modifier(rating)
            .map_err(|_| CastRejection::InvalidState)?;
        bace_magic::gdle_heal(current, raw * elemental, modifier, vital == Vital::Health)
            .map_err(|_| CastRejection::InvalidState)
    }
    pub(super) fn native_transfer(
        &self,
        actor: EntityId,
        src: EntityId,
        dst: EntityId,
        source_vital: Vital,
    ) -> Result<(f64, f64), CastRejection> {
        let mut drain = 1.;
        let mut boost = 1.;
        let index = match source_vital {
            Vital::Health => 8,
            Vital::Stamina => 9,
            Vital::Mana => 10,
        };
        if src != actor {
            let source = self.live_damage_profile(src)?;
            let q = source.resistances[index].quality;
            drain = (q.raw * q.increasing * q.decreasing
                + q.additive_increasing
                + q.additive_decreasing)
                .max(0.);
            if source.player {
                drain = drain.min(f64::from(
                    bace_magic::gdle_natural_resistance(
                        source.base_strength,
                        source.base_endurance,
                    )
                    .map_err(|_| CastRejection::InvalidState)?,
                ));
            }
        }
        if dst != actor {
            let target = self.live_damage_profile(dst)?;
            // Source switches on the source vital, and uses stamina/mana drain
            // qualities even for the receiving side of a transfer.
            boost = if index == 8 {
                target.boost_resistances[0]
            } else {
                let q = target.resistances[index].quality;
                (q.raw * q.increasing * q.decreasing
                    + q.additive_increasing
                    + q.additive_decreasing)
                    .max(0.)
            };
        }
        Ok((drain, boost))
    }
    pub(super) fn step_instant_continuations(
        &mut self,
        world: &mut World,
        now: f64,
        policy: &Combat,
        fellowships: &crate::fellowships::Fellowships,
        observers: Option<(&crate::characters::Characters, u64)>,
    ) {
        let mut pending = std::mem::take(&mut self.instant_continuations);
        while let Some((cast, mut attempt)) = pending.pop_first() {
            if !self.can_accept() {
                self.instant_continuations.insert(cast, attempt);
                self.instant_continuations.append(&mut pending);
                break;
            }
            if attempt.terminal.is_some() {
                self.flush_terminal(&mut attempt, world);
                if attempt.terminal.is_some() {
                    self.instant_continuations.insert(cast, attempt);
                }
                continue;
            }
            if attempt.portal_request_sent {
                self.instant_continuations.insert(cast, attempt);
                continue;
            }
            match self.release(&mut attempt, world, now, policy, fellowships, observers) {
                Ok(true) => {
                    self.publish_outcome(attempt.origin, Ok(CastChange::Completed { cast }))
                }
                Ok(false) => {
                    self.instant_continuations.insert(cast, attempt);
                }
                Err(error) => self.publish_outcome(attempt.origin, Err(error)),
            }
        }
    }
}
