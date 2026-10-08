//! Connected prepared physical attacks. One owner, immutable equipment snapshots,
//! bounded hook/cleave output and keyed random domains; no client pose authority.
use super::*;
use bace_combat::physical::{
    PhysicalDamageInput, SelectedPhysicalAttack, resolve_physical, select_melee,
    validate_physical_profile,
};
use bace_gameplay_api::weapon_combat::*;
use std::sync::Arc;
pub(crate) struct PhysicalAttackState {
    pub(super) operation: u64,
    pub(super) motion: Option<super::motion::PhysicalMotion>,
    pub(super) profile: Arc<PhysicalCombatProfile>,
    pub(super) selected: SelectedPhysicalAttack,
    pub(super) target: EntityId,
    credited_owner: EntityId,
    pub(super) height: u32,
    pub(super) power: f32,
    start: f64,
    next_hook: usize,
    pub(super) pending_targets: Option<Box<([Option<EntityId>; 32], usize)>>,
    pub(super) hook_cost: Option<(u32, f32)>,
    completed_targets: u32,
    stamina_spent: bool,
    pending_angles: [f32; 32],
    random: bace_random::RandomStream,
    pub(super) faulted: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PhysicalCombatEvent {
    ModeChanged {
        actor: EntityId,
        actor_incarnation: u64,
        mode: u32,
    },
    AttackDone {
        actor: EntityId,
        actor_incarnation: u64,
    },
    ProjectileCreated {
        proposal: PhysicalLaunchProposal,
        tick: u64,
        launch: Arc<bace_gameplay_api::visibility::AcceptedProjectileLaunch>,
        appearance: Option<Arc<bace_content::WeenieV1>>,
    },
    CommenceAttack {
        actor: EntityId,
        actor_incarnation: u64,
    },
    MotionHook {
        actor: EntityId,
        event: bace_motion::MotionExecutionEvent,
    },
    ProjectileDestroy {
        actor: EntityId,
        tick: u64,
    },
    ProjectileResting {
        actor: EntityId,
        tick: u64,
    },
    ProjectileRemoved {
        actor: EntityId,
        tick: u64,
    },
    Impact {
        attacker: EntityId,
        attacker_incarnation: u64,
        target_incarnation: u64,
        target: EntityId,
        impact: PhysicalImpact,
        attacker_name: Arc<str>,
        target_name: Arc<str>,
        applied: u32,
        current: u32,
        maximum: u32,
        revision: u64,
    },
    Motion {
        actor: EntityId,
        target: EntityId,
        motion: u32,
        speed: f32,
    },
    Fault {
        actor: EntityId,
        actor_incarnation: u64,
        target: EntityId,
        operation: u64,
        error: CombatRejection,
    },
}
impl Combat {
    pub(crate) fn has_physical_state(&self) -> bool {
        !self.physical_hits.is_empty()
            || !self.physical.is_empty()
            || !self.physical_attacks.is_empty()
            || !self.physical_events.is_empty()
            || !self.missiles.is_empty()
            || !self.launches.is_empty()
            || !self.missile_ids.is_empty()
    }
    pub(crate) fn register_physical(
        &mut self,
        actor: EntityId,
        profile: Arc<PhysicalCombatProfile>,
        world: &World,
    ) -> Result<(), CombatRejection> {
        validate_physical_profile(&profile).map_err(|_| CombatRejection::InvalidRequest)?;
        if world.combatant(actor).is_none() {
            return Err(CombatRejection::MissingActor);
        }
        if self.attacks.contains_key(&actor)
            || self.physical_attacks.contains_key(&actor)
            || self
                .missiles
                .values()
                .any(|m| m.proposal.actor == actor.0 && !m.launched)
        {
            return Err(CombatRejection::Busy);
        }
        if !self.physical.contains_key(&actor) && self.physical.len() >= self.capacity {
            return Err(CombatRejection::Capacity);
        }
        if profile
            .main
            .as_ref()
            .is_some_and(|w| w.cleave_targets as usize > self.capacity)
            || profile
                .offhand
                .as_ref()
                .is_some_and(|w| w.cleave_targets as usize > self.capacity)
        {
            return Err(CombatRejection::Capacity);
        }
        let mut ratings = profile.ratings;
        if let Some(old) = self.physical_ratings.get(&actor) {
            ratings.reckless = old.reckless;
            ratings.sneak = old.sneak;
        }
        self.physical_ratings.insert(actor, ratings);
        self.physical.insert(actor, profile);
        Ok(())
    }
    pub(crate) fn take_physical_event(&mut self) -> Option<PhysicalCombatEvent> {
        self.physical_events.pop_front()
    }
    pub(crate) fn physical_profile(&self, actor: EntityId) -> Option<&Arc<PhysicalCombatProfile>> {
        self.physical.get(&actor)
    }
    pub(super) fn start_physical(
        &mut self,
        world: &mut World,
        actor: EntityId,
        target: EntityId,
        height: u32,
        power: f32,
        now: f64,
    ) -> Result<CombatChange, CombatRejection> {
        if !now.is_finite() || now < 0.0 {
            return Err(CombatRejection::InvalidRequest);
        }
        if self
            .physical_deadlines
            .get(&actor)
            .is_some_and(|until| now < *until)
        {
            return Err(CombatRejection::Busy);
        }
        if self.attacks.contains_key(&actor)
            || self.physical_attacks.contains_key(&actor)
            || self
                .missiles
                .values()
                .any(|m| m.proposal.actor == actor.0 && !m.launched)
        {
            return Err(CombatRejection::Busy);
        }
        if self.physical_attacks.len() >= self.capacity {
            return Err(CombatRejection::Capacity);
        }
        let profile = self
            .physical
            .get(&actor)
            .ok_or(CombatRejection::MissingCombatProfile)?
            .clone();
        let target_profile = self
            .physical
            .get(&target)
            .ok_or(CombatRejection::MissingCombatProfile)?;
        if !runtime_permission(world, actor, target, &profile, target_profile) {
            return Err(CombatRejection::InvalidRequest);
        }
        let selected = select_melee(
            &profile,
            height,
            power,
            self.offhand.get(&actor).copied().unwrap_or(false),
        )
        .map_err(|_| CombatRejection::InvalidRequest)?;
        let deadline = now
            + self
                .physical_chain(
                    world,
                    actor,
                    profile.maneuvers[selected.maneuver].motion,
                    selected.speed,
                )
                .map_or(
                    profile.maneuvers[selected.maneuver].duration / f64::from(selected.speed),
                    |c| c.nominal_duration_seconds(),
                );
        if !deadline.is_finite() || deadline <= now {
            return Err(CombatRejection::InvalidRequest);
        }
        let (near, clear) = world
            .attack_geometry(actor, target, profile.range)
            .map_err(|_| CombatRejection::MissingActor)?;
        if !near {
            return Err(CombatRejection::OutOfRange);
        }
        if !clear {
            return Err(CombatRejection::Obstructed);
        }
        let (root, epoch) = self
            .random
            .as_ref()
            .ok_or(CombatRejection::InvalidRequest)?;
        let serial = self
            .next_attack
            .checked_add(1)
            .ok_or(CombatRejection::Capacity)?;
        let mut id = [0; 16];
        id[..8].copy_from_slice(&epoch.to_le_bytes());
        id[8..].copy_from_slice(&serial.to_le_bytes());
        let random = root
            .event_stream(id, bace_random::Domain::Combat)
            .map_err(|_| CombatRejection::InvalidRequest)?;
        if self.physical_events.len() == self.capacity.max(2) {
            return Err(CombatRejection::Capacity);
        }
        let motion = self.begin_physical_motion(
            world,
            actor,
            serial,
            profile.maneuvers[selected.maneuver].motion,
            selected.speed,
        )?;
        self.physical_events.push_back(PhysicalCombatEvent::Motion {
            actor,
            target,
            motion: profile.maneuvers[selected.maneuver].motion,
            speed: selected.speed,
        });
        self.next_attack = serial;
        self.physical_deadlines.insert(actor, deadline);
        self.physical_attacks.insert(
            actor,
            PhysicalAttackState {
                operation: serial,
                motion,
                profile,
                selected,
                target,
                credited_owner: world.damage_owner(actor),
                height,
                power,
                start: now,
                next_hook: 0,
                pending_targets: None,
                hook_cost: None,
                completed_targets: 0,
                stamina_spent: false,
                pending_angles: [0.0; 32],
                random,
                faulted: false,
            },
        );
        Ok(CombatChange::AttackStarted { target })
    }
    pub(super) fn step_physical(
        &mut self,
        world: &mut World,
        now: f64,
        inventory: Option<&crate::inventory::Inventory>,
        only_pending: bool,
    ) {
        let mut remove = None;
        for (&actor, attack) in &mut self.physical_attacks {
            if only_pending
                && attack.hook_cost.is_none()
                && !self
                    .physical_hits
                    .keys()
                    .any(|key| key.attacker == actor && key.operation == attack.operation)
            {
                continue;
            }
            if attack.faulted {
                continue;
            }
            if inventory
                .is_some_and(|i| !super::equipment::equipment_current(&attack.profile, actor, i))
            {
                if self.physical_events.len() == self.capacity.max(2) {
                    return;
                }
                self.physical_events.push_back(PhysicalCombatEvent::Fault {
                    actor,
                    actor_incarnation: incarnation(world, actor),
                    target: attack.target,
                    operation: 0,
                    error: CombatRejection::MissingCombatProfile,
                });
                attack.faulted = true;
                continue;
            }
            let maneuver = &attack.profile.maneuvers[attack.selected.maneuver];
            let alive = world
                .combatant(actor)
                .is_some_and(|v| v.health() > 0 && v.mode() == 2)
                && world
                    .combatant(attack.target)
                    .is_some_and(|v| v.health() > 0);
            let motion_valid = attack.motion.as_ref().is_none_or(|m| {
                m.complete != Some(false)
                    && world
                        .actor_state(actor)
                        .is_ok_and(|(_, s)| s.epoch() == m.epoch)
            });
            let retained = attack.hook_cost.is_some()
                || self
                    .physical_hits
                    .keys()
                    .any(|k| k.attacker == actor && k.operation == attack.operation);
            if (!alive || !motion_valid) && !retained {
                if self.physical_events.len() == self.capacity.max(2) {
                    break;
                }
                self.physical_events
                    .push_back(PhysicalCombatEvent::AttackDone {
                        actor,
                        actor_incarnation: incarnation(world, actor),
                    });
                remove = Some(actor);
                break;
            }
            let hook = if let Some(motion) = &attack.motion {
                motion
                    .hooks
                    .front()
                    .map(|&part| PhysicalAttackHook { seconds: 0.0, part })
            } else {
                maneuver.hooks.get(attack.next_hook).copied()
            };
            let Some(hook) = hook else {
                let completed = attack.motion.as_ref().map_or(
                    now >= attack.start + maneuver.duration / f64::from(attack.selected.speed),
                    |m| m.complete.is_some(),
                );
                if completed && self.physical_events.len() < self.capacity.max(2) {
                    if let Some(m) = &attack.motion
                        && super::faults::end_if_owned(world, actor, m.token.owner).is_err()
                    {
                        continue;
                    }
                    self.offhand.insert(
                        actor,
                        !matches!(attack.selected.hand, PhysicalHand::Offhand),
                    );
                    self.physical_events
                        .push_back(PhysicalCombatEvent::AttackDone {
                            actor,
                            actor_incarnation: incarnation(world, actor),
                        });
                    remove = Some(actor);
                    break;
                }
                continue;
            };
            if attack.motion.is_none()
                && now < attack.start + hook.seconds / f64::from(attack.selected.speed)
            {
                continue;
            }
            let mut targets = [None; 32];
            targets[0] = Some(attack.target);
            let mut count = 1;
            if let Some(pending) = &attack.pending_targets {
                (targets, count) = **pending;
            }
            let weapon = if attack.selected.hand == PhysicalHand::Offhand {
                &attack.profile.offhand
            } else {
                &attack.profile.main
            };
            let cleaves = weapon.as_ref().map_or(1, |w| w.cleave_targets.max(1)) as usize;
            if cleaves > 1 && attack.pending_targets.is_none() {
                for (id, _, _) in world.states() {
                    if count >= cleaves {
                        break;
                    }
                    if id == actor || id == attack.target {
                        continue;
                    }
                    let Some(profile) = self.physical.get(&id) else {
                        continue;
                    };
                    if !attack.profile.player && !profile.player
                        || !runtime_permission(world, actor, id, &attack.profile, profile)
                        || world.combatant(id).is_none_or(|s| s.health() == 0)
                    {
                        continue;
                    }
                    if !matches!(
                        world.attack_geometry(actor, id, attack.profile.range),
                        Ok((true, true))
                    ) {
                        continue;
                    }
                    // GDLE CLEAVING_ATTACK_ANGLE compares bearing from the attacker.
                    let angle = accepted_angle(world, id, actor).unwrap_or(180.0);
                    if angle.abs() >= 178.0 {
                        continue;
                    }
                    targets[count] = Some(id);
                    count += 1;
                }
            }
            if std::iter::once(actor)
                .chain(targets[..count].iter().flatten().copied())
                .any(|id| {
                    world.vital_reserved(id, bace_entity::EntityVital::Health)
                        || world.vital_reserved(id, bace_entity::EntityVital::Stamina)
                })
            {
                if attack.pending_targets.is_none() {
                    attack.pending_targets = Some(Box::new((targets, count)));
                }
                continue;
            }
            if self.events.len() + count > self.capacity
                || self.physical_events.len() + count > self.capacity.max(2)
                || self.dirty.len() + count > self.capacity
            {
                if attack.pending_targets.is_none() {
                    attack.pending_targets = Some(Box::new((targets, count)));
                }
                break;
            }
            let mut source_ratings = self
                .physical_ratings
                .get(&actor)
                .copied()
                .unwrap_or(attack.profile.ratings);
            let mut decisions = [None; 32];
            let mut invalid = false;
            let mut pending_proc = false;
            let mut new_contacts = [false; 32];
            let mut defense_costs = [0u32; 32];
            let Some(source_stamina) = world
                .combatant(actor)
                .and_then(|s| s.vital(bace_entity::EntityVital::Stamina))
            else {
                if self.physical_events.len() < self.capacity.max(2) {
                    self.physical_events.push_back(PhysicalCombatEvent::Fault {
                        actor,
                        actor_incarnation: incarnation(world, actor),
                        target: attack.target,
                        operation: 0,
                        error: CombatRejection::MissingCombatProfile,
                    });
                    attack.faulted = true;
                }
                continue;
            };
            let stamina_roll = impact_rolls(&attack.random, attack.next_hook, actor)
                .map(|r| r.attacker_stamina)
                .unwrap_or(0.0);
            let cost =
                bace_combat::physical::attack_stamina(bace_combat::physical::AttackStaminaInput {
                    kind: PhysicalKind::Melee,
                    player: attack.profile.player,
                    base_endurance: attack.profile.base_endurance,
                    weapon_burden: weapon.as_ref().map_or(0, |w| w.encumbrance),
                    shield_burden: attack.profile.shield_encumbrance,
                    shield_placement: attack.profile.shield_placement,
                    power: if attack.profile.player {
                        attack.power
                    } else {
                        0.5
                    },
                    roll: stamina_roll,
                })
                .unwrap_or(u32::MAX);
            let impact_power = if source_stamina.current < cost {
                0.0
            } else if attack.profile.player {
                attack.power
            } else {
                0.5
            };
            let (cost, impact_power) = attack.hook_cost.unwrap_or((cost, impact_power));
            if self.physical_procs_enabled && attack.hook_cost.is_none() {
                attack.hook_cost = Some((cost, impact_power));
                attack.pending_targets = Some(Box::new((targets, count)));
                for (index, target) in targets[..count].iter().enumerate() {
                    attack.pending_angles[index] =
                        accepted_angle(world, actor, target.expect("selected")).unwrap_or(0.0);
                }
            }
            let active_target =
                (0..count).find(|index| attack.completed_targets & (1u32 << index) == 0);
            if world
                .combatant(actor)
                .is_none_or(|s| s.revision() == u64::MAX)
            {
                invalid = true;
            }

            for index in 0..count {
                if self.physical_procs_enabled && Some(index) != active_target {
                    continue;
                }
                let target = targets[index].expect("bounded selected target");
                let Some(defender) = self.physical.get(&target) else {
                    invalid = true;
                    break;
                };
                if inventory
                    .is_some_and(|i| !super::equipment::equipment_current(defender, target, i))
                {
                    invalid = true;
                    break;
                }

                if !retained
                    && !matches!(
                        world.attack_geometry(actor, target, attack.profile.range),
                        Ok((true, true))
                    )
                {
                    // A target may leave the accepted cone while the authored
                    // swing is running. The hook is spent; it cannot be retried
                    // against a later pose or converted into a delayed hit.
                    continue;
                }
                let Some(state) = world.combatant(target) else {
                    invalid = true;
                    break;
                };
                let rolls = match impact_rolls(&attack.random, attack.next_hook, target) {
                    Ok(v) => v,
                    Err(_) => {
                        invalid = true;
                        break;
                    }
                };
                let input = PhysicalDamageInput {
                    attacker: &attack.profile,
                    pk_override: Some((
                        live_pk(world, actor, attack.profile.pk),
                        live_pk(world, target, defender.pk),
                    )),
                    rating_override: Some((
                        source_ratings,
                        self.physical_ratings
                            .get(&target)
                            .copied()
                            .unwrap_or(defender.ratings),
                    )),
                    defender,
                    selected: attack.selected,
                    kind: PhysicalKind::Melee,
                    hook_part: hook.part,
                    height: attack.height,
                    original_power: attack.power,
                    power: impact_power,
                    target_angle_degrees: f64::from(if self.physical_procs_enabled {
                        attack.pending_angles[index]
                    } else {
                        accepted_angle(world, actor, target).unwrap_or(0.0)
                    }),
                    defender_stamina: state
                        .vital(bace_entity::EntityVital::Stamina)
                        .map_or(0, |v| v.current),
                    defender_in_combat: state.mode() != 1,
                    rolls,
                };
                let key = bace_gameplay_api::physical_procs::PhysicalHitKey {
                    attacker: actor,
                    operation: attack.operation,
                    ordinal: attack.next_hook as u32,
                    target,
                    kind: PhysicalKind::Melee,
                };
                let impact = if self.physical_procs_enabled {
                    let draws =
                        match super::procs::proc_draws(&attack.random, attack.next_hook, target) {
                            Ok(draws) => draws,
                            Err(_) => {
                                invalid = true;
                                break;
                            }
                        };
                    let existed = self.physical_hits.contains_key(&key);
                    let progressed = super::procs::advance_hit(
                        &mut self.physical_hits,
                        self.capacity,
                        key,
                        input,
                        weapon.as_ref().map(|w| EntityId(w.entity)),
                        draws,
                    );
                    new_contacts[index] = !existed && self.physical_hits.contains_key(&key);
                    match progressed {
                        Ok(Some(impact)) => Ok(impact),
                        Ok(None) => {
                            pending_proc = true;
                            continue;
                        }
                        Err(_) => Err(bace_combat::physical::PhysicalError::InvalidInput),
                    }
                } else {
                    resolve_physical(input)
                };
                let Ok(impact) = impact else {
                    invalid = true;
                    break;
                };
                let Some(stamina) = state.vital(bace_entity::EntityVital::Stamina) else {
                    invalid = true;
                    break;
                };
                defense_costs[index] = if stamina.current == 0 {
                    0
                } else {
                    bace_combat::physical::defense_stamina(
                        defender.player,
                        impact.evaded,
                        defender
                            .skills
                            .iter()
                            .find(|s| s.0 == 6)
                            .map_or(0, |s| s.1.advancement),
                        defender.base_endurance,
                        rolls.defender_stamina,
                    )
                    .unwrap_or(1)
                };
                if !state.can_record_damage(attack.credited_owner)
                    || state.revision() > u64::MAX - 2
                {
                    invalid = true;
                    break;
                }
                source_ratings.reckless = impact.attacker_reckless_rating;
                source_ratings.sneak = impact.attacker_sneak_rating;
                decisions[index] = Some(impact);
            }
            for index in 0..count {
                if new_contacts[index] {
                    crate::pk_activity::record_pair(
                        world,
                        actor,
                        targets[index].expect("contact"),
                        now,
                    );
                }
            }
            if pending_proc {
                attack.pending_targets = Some(Box::new((targets, count)));
                continue;
            }
            if world
                .validate_health_observations(targets[..count].iter().flatten().copied())
                .is_err()
            {
                continue;
            }
            let mut names: [Option<Arc<str>>; 32] = std::array::from_fn(|_| None);
            let attacker_name = if !invalid {
                capture_combat_name(world, actor).ok()
            } else {
                None
            };
            if attacker_name.is_none() {
                invalid = true;
            }
            if !invalid {
                for index in 0..count {
                    if decisions[index].is_some() {
                        names[index] =
                            capture_combat_name(world, targets[index].expect("target")).ok();
                        if names[index].is_none() {
                            invalid = true;
                            break;
                        }
                    }
                }
            }
            if invalid {
                if !retained {
                    attack.hook_cost = None;
                    attack.pending_targets = None;
                    self.physical_hits.retain(|key, _| {
                        !(key.attacker == actor
                            && key.operation == attack.operation
                            && key.ordinal == attack.next_hook as u32)
                    });
                }
                self.physical_events.push_back(PhysicalCombatEvent::Fault {
                    actor,
                    actor_incarnation: incarnation(world, actor),
                    target: attack.target,
                    operation: 0,
                    error: CombatRejection::InvalidRequest,
                });
                attack.faulted = true;
                break;
            }
            let attacker_incarnation = incarnation(world, actor);
            if !attack.stamina_spent {
                world
                    .combatant_mut(actor)
                    .expect("preflight")
                    .apply_vital(
                        bace_entity::EntityVital::Stamina,
                        source_stamina.current,
                        source_stamina.current.saturating_sub(cost),
                        None,
                    )
                    .expect("preflight");
                attack.stamina_spent = true;
            }
            for index in 0..count {
                let target = targets[index].expect("selected");
                let Some(impact) = decisions[index] else {
                    continue;
                };
                // GDLE melee marks the accepted hostile attempt even on evade.
                // Pure decisions were preflighted for the whole cleave first.
                if !self.physical_procs_enabled {
                    crate::pk_activity::record_pair(world, actor, target, now);
                }
                let amount = world
                    .damage_from(target, attack.credited_owner, impact.damage)
                    .expect("same owner preflight");
                let state = world.combatant_mut(target).expect("preflight");
                let stamina = state
                    .vital(bace_entity::EntityVital::Stamina)
                    .expect("preflight");
                state
                    .apply_vital(
                        bace_entity::EntityVital::Stamina,
                        stamina.current,
                        stamina.current.saturating_sub(defense_costs[index]),
                        None,
                    )
                    .expect("preflight");
                if !self.physical_procs_enabled && impact.dirty_count > 0 {
                    self.dirty.push_back(DirtyFightingImpact {
                        attacker: actor,
                        target,
                        spells: impact.dirty_spells,
                        count: impact.dirty_count,
                    });
                }
                if let Some(rating) = self.physical_ratings.get_mut(&actor) {
                    rating.reckless = impact.attacker_reckless_rating;
                    rating.sneak = impact.attacker_sneak_rating;
                }
                self.physical_events.push_back(PhysicalCombatEvent::Impact {
                    attacker: actor,
                    attacker_incarnation,
                    target_incarnation: state.incarnation(),
                    target,
                    impact,
                    attacker_name: attacker_name.as_ref().expect("preflighted name").clone(),
                    target_name: names[index].take().expect("preflighted name"),
                    applied: amount,
                    current: state.health(),
                    maximum: state.profile().maximum_health,
                    revision: state.revision(),
                });
                self.events.push_back(CombatEvent::Damage {
                    target_incarnation: state.incarnation(),
                    attacker: Some(actor),
                    death_blow: Some(super::DeathBlow {
                        damage_type: impact.damage_type,
                        critical: impact.critical,
                    }),
                    target,
                    amount,
                    current: state.health(),
                    maximum: state.profile().maximum_health,
                    killed: amount > 0 && state.health() == 0,
                    revision: state.revision(),
                });
            }
            if self.physical_procs_enabled {
                let index = active_target.expect("remaining selected target");
                let target = targets[index].expect("selected");
                self.physical_hits.retain(|key, _| {
                    !(key.attacker == actor
                        && key.operation == attack.operation
                        && key.ordinal == attack.next_hook as u32
                        && key.target == target)
                });
                attack.completed_targets |= 1u32 << index;
                if (0..count).any(|index| attack.completed_targets & (1u32 << index) == 0) {
                    continue;
                }
            }
            attack.completed_targets = 0;
            attack.stamina_spent = false;
            attack.hook_cost = None;
            attack.next_hook += 1;
            attack.pending_targets = None;
            if let Some(motion) = &mut attack.motion {
                motion.hooks.pop_front();
            }
        }
        if let Some(actor) = remove {
            self.physical_attacks.remove(&actor);
        }
    }
}
pub(super) fn impact_rolls(
    stream: &bace_random::RandomStream,
    hook: usize,
    target: EntityId,
) -> Result<PhysicalRolls, CombatRejection> {
    let mut s = stream
        .fork(
            b"physical-impact",
            ((hook as u64) << 32) | u64::from(target.0),
        )
        .map_err(|_| CombatRejection::InvalidRequest)?;
    let mut draw = || {
        s.next_u64()
            .map(|v| (v >> 11) as f64 / 9_007_199_254_740_992.0)
            .map_err(|_| CombatRejection::InvalidRequest)
    };
    Ok(PhysicalRolls {
        attacker_stamina: draw()?,
        defender_stamina: draw()?,
        evade: draw()?,
        critical: draw()?,
        variance: (draw()? as f32).min(f32::from_bits(0x3f7fffff)),
        body_part: (draw()? as f32).min(f32::from_bits(0x3f7fffff)),
        critical_defense: (draw()? as f32).min(f32::from_bits(0x3f7fffff)),
        sneak: (draw()? as f32).min(f32::from_bits(0x3f7fffff)),
        dirty: draw()?,
        weapon_proc: draw()?,
    })
}
impl Combat {
    pub(crate) fn spell_permission(
        &self,
        source: EntityId,
        target: EntityId,
        helpful: bool,
        world: &World,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        use bace_gameplay_api::CastRejection as E;
        let source_state = world.combatant(source).ok_or(E::MissingActor)?;
        let target_state = world.combatant(target).ok_or(E::MissingActor)?;
        if source == target {
            return Ok(());
        }
        if !helpful && target_state.lifestone_protected() {
            return Err(E::InvalidTarget);
        }
        match (self.physical.get(&source), self.physical.get(&target)) {
            (Some(a), Some(b)) => {
                if !helpful && (!b.attackable || b.immune || b.lifestone_protected) {
                    return Err(E::InvalidTarget);
                }
                if a.player && b.player {
                    let allowed = if helpful {
                        bace_combat::physical::pk_action_allowed(
                            true,
                            live_pk(world, source, a.pk),
                            live_pk(world, target, b.pk),
                        )
                    } else {
                        runtime_permission(world, source, target, a, b)
                    };
                    if !allowed {
                        return Err(E::InvalidTarget);
                    }
                }
                Ok(())
            }
            _ if source_state.profile().player && target_state.profile().player => {
                Err(E::InvalidTarget)
            }
            _ => Ok(()),
        }
    }
}

pub(super) fn live_pk(world: &World, actor: EntityId, fallback: PkStatus) -> PkStatus {
    if let Some(value) = world.combatant(actor).and_then(|c| c.death_pk_status()) {
        return match value {
            2 => PkStatus::Npk,
            4 => PkStatus::Pk,
            64 => PkStatus::PkLite,
            32 => PkStatus::Free,
            16 => PkStatus::RubberGlue,
            _ => PkStatus::Protected,
        };
    }

    match world
        .properties(actor)
        .and_then(|p| p.get(bace_entity::PropertyFamily::Int, 134))
    {
        Some(bace_entity::PropertyValue::Int(v)) => match v {
            2 => PkStatus::Npk,
            4 => PkStatus::Pk,
            64 => PkStatus::PkLite,
            32 => PkStatus::Free,
            16 => PkStatus::RubberGlue,
            _ => PkStatus::Protected,
        },
        _ => fallback,
    }
}
pub(super) fn runtime_permission(
    world: &World,
    a: EntityId,
    b: EntityId,
    source: &PhysicalCombatProfile,
    target: &PhysicalCombatProfile,
) -> bool {
    bace_combat::physical::physical_permission_with_status(
        source,
        target,
        live_pk(world, a, source.pk),
        live_pk(world, b, target.pk),
    )
}

pub(super) fn capture_combat_name(
    world: &World,
    actor: EntityId,
) -> Result<Arc<str>, CombatRejection> {
    match world
        .properties(actor)
        .and_then(|p| p.get(bace_entity::PropertyFamily::String, 1))
    {
        Some(bace_entity::PropertyValue::String(name)) if name.len() <= 1024 => {
            Ok(Arc::from(name.as_str()))
        }
        None => Ok(Arc::from("Unknown")),
        _ => Err(CombatRejection::InvalidRequest),
    }
}
