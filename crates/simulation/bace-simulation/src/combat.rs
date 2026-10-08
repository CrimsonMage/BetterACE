mod admission;
mod driver;
mod equipment;
mod equipment_admission;
mod faults;
mod missile;
mod motion;
mod physical;
mod procs;
mod refresh;
#[cfg(test)]
mod reservation_tests;
mod residency;
mod specializations;
use bace_combat::{AttackImpact, MeleeAttack, Strike};
use bace_gameplay_api::{CombatChange, CombatRejection, CombatRequest};
use bace_types::EntityId;
use bace_world::World;
pub use physical::PhysicalCombatEvent;
use specializations::{CombatSkills, ImpactInputs, accepted_angle, damage_and_dirty};
pub use specializations::{
    DirtyFightingImpact, PreparedAttributeModifier, PreparedCharacterSkillInputs, PreparedShield,
    SkillRefreshError,
};
use std::collections::{BTreeMap, VecDeque};
mod ammunition_pool;

/// Authoritative state deltas; damage/death are one event so output pressure
/// cannot publish damage without its associated terminal transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathBlow {
    /// The selected ACE DamageType flag, captured at the accepted impact.
    pub damage_type: u32,
    pub critical: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatEvent {
    Damage {
        target_incarnation: u64,
        attacker: Option<EntityId>,
        death_blow: Option<DeathBlow>,
        target: EntityId,
        amount: u32,
        current: u32,
        maximum: u32,
        killed: bool,
        revision: u64,
    },
    Finished {
        actor: EntityId,
        actor_incarnation: u64,
        cancelled: bool,
    },
}
struct ActiveAttack {
    credited_owner: EntityId,
    attack: MeleeAttack,
    power_modifier: f32,
    power: f32,
    height: u32,
    random: Option<bace_random::RandomStream>,
}
pub(crate) struct Combat {
    attacks: BTreeMap<EntityId, ActiveAttack>,
    physical:
        BTreeMap<EntityId, std::sync::Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>>,
    physical_procs_enabled: bool,
    physical_hits:
        BTreeMap<bace_gameplay_api::physical_procs::PhysicalHitKey, procs::PendingPhysicalHit>,
    physical_attacks: BTreeMap<EntityId, physical::PhysicalAttackState>,
    physical_events: VecDeque<PhysicalCombatEvent>,
    physical_ratings: BTreeMap<EntityId, bace_gameplay_api::weapon_combat::PhysicalRatings>,
    offhand: BTreeMap<EntityId, bool>,
    /// Prepared animation admission deadlines survive cancel, mode changes and early impacts.
    physical_deadlines: BTreeMap<EntityId, f64>,
    physical_motions:
        BTreeMap<motion::PhysicalMotionKey, std::sync::Arc<bace_motion::PreparedMotionChain>>,
    physical_motion_pending: Option<bace_world::WorldMotionEvent>,
    physical_options: BTreeMap<EntityId, (u32, u32)>,
    physical_sources:
        BTreeMap<EntityId, std::sync::Arc<bace_combat::preparation::PreparedPhysicalRefreshSource>>,
    physical_drivers: BTreeMap<EntityId, driver::AttackDriver>,
    physical_driver_cursor: Option<EntityId>,
    physical_approach_scratch: Vec<EntityId>,
    simulation_now: f64,
    simulation_tick: u64,
    missiles: BTreeMap<u64, missile::MissileAttackState>,
    launches: VecDeque<bace_gameplay_api::weapon_combat::PhysicalLaunchProposal>,
    missile_ids: VecDeque<EntityId>,
    events: VecDeque<CombatEvent>,
    capacity: usize,
    pub(crate) skills: BTreeMap<EntityId, CombatSkills>,
    random: Option<(std::sync::Arc<bace_random::RandomRoot>, u64)>,
    next_attack: u64,
    dirty: VecDeque<DirtyFightingImpact>,
    pub(crate) refresh_scratch: Vec<EntityId>,
}
impl Combat {
    pub(crate) fn set_simulation_tick(&mut self, tick: u64) {
        self.simulation_tick = tick;
    }
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            attacks: BTreeMap::new(),
            physical: BTreeMap::new(),
            physical_procs_enabled: false,
            physical_hits: BTreeMap::new(),
            physical_attacks: BTreeMap::new(),
            physical_events: VecDeque::with_capacity(capacity.max(2)),
            physical_ratings: BTreeMap::new(),
            offhand: BTreeMap::new(),
            physical_deadlines: BTreeMap::new(),
            physical_motions: BTreeMap::new(),
            physical_motion_pending: None,
            physical_options: BTreeMap::new(),
            physical_sources: BTreeMap::new(),
            physical_drivers: BTreeMap::new(),
            physical_driver_cursor: None,
            physical_approach_scratch: Vec::with_capacity(capacity),
            simulation_now: 0.0,
            simulation_tick: 0,
            missiles: BTreeMap::new(),
            launches: VecDeque::with_capacity(capacity),
            missile_ids: VecDeque::with_capacity(capacity),
            events: VecDeque::with_capacity(capacity),
            capacity,
            skills: BTreeMap::new(),
            random: None,
            next_attack: 0,
            dirty: VecDeque::new(),
            refresh_scratch: Vec::with_capacity(capacity),
        }
    }
    pub(crate) fn prepare_physical_admission(
        &self,
        profiles: &[(
            EntityId,
            std::sync::Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
        )],
    ) -> Result<(), CombatRejection> {
        if profiles.len() > self.capacity - self.physical.len() {
            return Err(CombatRejection::Capacity);
        }
        for (index, (id, profile)) in profiles.iter().enumerate() {
            bace_combat::physical::validate_physical_profile(profile)
                .map_err(|_| CombatRejection::InvalidRequest)?;
            if self.physical.contains_key(id)
                || self.attacks.contains_key(id)
                || self.physical_attacks.contains_key(id)
                || profiles[..index].iter().any(|(old, _)| old == id)
            {
                return Err(CombatRejection::Busy);
            }
        }
        Ok(())
    }
    pub(crate) fn adopt_physical_admission(
        &mut self,
        profiles: Vec<(
            EntityId,
            std::sync::Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
        )>,
    ) {
        for (id, profile) in profiles {
            self.physical_ratings.insert(id, profile.ratings);
            self.physical.insert(id, profile);
        }
    }
    /// Accepted entity retirement removes profiles without discarding already
    /// accepted damage/launch outputs or projectiles that are still in flight.
    pub(crate) fn retire_actor(&mut self, actor: EntityId) {
        if self.physical_proc_pending(actor) {
            return;
        }
        self.attacks.remove(&actor);
        self.physical_attacks.remove(&actor);
        self.physical.remove(&actor);
        self.physical_ratings.remove(&actor);
        self.offhand.remove(&actor);
        self.physical_deadlines.remove(&actor);
        self.physical_motions
            .retain(|(id, _, _, _, _, _), _| *id != actor);
        self.physical_options.remove(&actor);
        self.physical_sources.remove(&actor);
        self.physical_drivers.remove(&actor);
        self.skills.remove(&actor);
    }
    pub(crate) fn cancel(&mut self, actor: EntityId) {
        if self.physical_proc_pending(actor) {
            return;
        }
        if let Some(driver) = self.physical_drivers.get_mut(&actor) {
            driver.cancel();
        }
        self.physical_attacks.remove(&actor);
        if let Some(attack) = self.attacks.get_mut(&actor) {
            attack.attack.cancel();
        }
    }
    pub(crate) fn take_event(&mut self) -> Option<CombatEvent> {
        self.events.pop_front()
    }
    pub(crate) fn pending_events(&self) -> usize {
        self.events.len()
    }
    pub(crate) fn peek_event(&self) -> Option<CombatEvent> {
        self.events.front().copied()
    }
    pub(crate) fn recovery_pending(&self, actor: EntityId, now: f64) -> bool {
        self.physical_deadlines
            .get(&actor)
            .is_some_and(|until| now < *until)
    }
    pub(crate) fn active(&self, actor: EntityId) -> bool {
        self.physical_proc_pending(actor)
            || self.attacks.contains_key(&actor)
            || self
                .physical_drivers
                .get(&actor)
                .is_some_and(driver::AttackDriver::active)
            || self.physical_attacks.contains_key(&actor)
            || self.missiles.values().any(|m| m.proposal.actor == actor.0)
    }
    pub(crate) fn apply(
        &mut self,
        world: &mut World,
        actor: EntityId,
        request: CombatRequest,
        now: f64,
    ) -> Result<CombatChange, CombatRejection> {
        let source = world
            .combatant(actor)
            .ok_or(CombatRejection::MissingCombatProfile)?;
        if source.health() == 0 {
            return Err(CombatRejection::Dead);
        }
        if matches!(
            request,
            CombatRequest::TargetedMelee { .. } | CombatRequest::TargetedMissile { .. }
        ) && world
            .body(actor)
            .is_ok_and(|b| b.collision_shape().is_some())
            && !self.physical_drivers.contains_key(&actor)
        {
            return Err(CombatRejection::MissingCombatProfile);
        }
        if self.physical_drivers.contains_key(&actor)
            && matches!(
                request,
                CombatRequest::TargetedMelee { .. } | CombatRequest::TargetedMissile { .. }
            )
        {
            return self.queue_physical_request(world, actor, request, now);
        }
        match request {
            CombatRequest::TargetedMissile {
                target,
                height,
                accuracy,
            } => self.start_missile(world, actor, target, height, accuracy, now),
            CombatRequest::ChangeMode(mode) => {
                if self.physical_proc_pending(actor) {
                    return Err(CombatRejection::Busy);
                }
                if mode != 1 && mode != 2 && mode != 4 && mode != 8 {
                    return Err(CombatRejection::UnsupportedMode);
                }
                let style = self.prepare_mode_style(world, actor, mode)?;
                if mode != source.mode()
                    && self
                        .missiles
                        .values()
                        .any(|m| m.proposal.actor == actor.0 && m.submitted && !m.launched)
                {
                    return Err(CombatRejection::Busy);
                }
                if mode != 4 {
                    self.missiles
                        .retain(|_, m| m.proposal.actor != actor.0 || m.submitted);
                }
                if mode != source.mode() {
                    self.stop_physical_approach(world, actor);
                    if let Some(driver) = self.physical_drivers.get_mut(&actor) {
                        driver.cancel();
                    }
                    if let Some(motion) = self
                        .physical_attacks
                        .get(&actor)
                        .and_then(|a| a.motion.as_ref())
                    {
                        faults::end_if_owned(world, actor, motion.token.owner)
                            .map_err(|_| CombatRejection::Busy)?;
                    }
                }
                self.request_mode_style(world, actor, style)?;
                world
                    .combatant_mut(actor)
                    .expect("checked actor")
                    .set_mode(mode);
                if mode != 2
                    && let Some(attack) = self.attacks.get_mut(&actor)
                {
                    attack.attack.cancel();
                }
                Ok(CombatChange::Mode(mode))
            }
            CombatRequest::CancelAttack => {
                if self.physical_proc_pending(actor) {
                    return Err(CombatRejection::Busy);
                }
                if self
                    .missiles
                    .values()
                    .any(|m| m.proposal.actor == actor.0 && m.submitted && !m.launched)
                {
                    return Err(CombatRejection::Busy);
                }
                if let Some(motion) = self
                    .physical_attacks
                    .get(&actor)
                    .and_then(|a| a.motion.as_ref())
                {
                    faults::end_if_owned(world, actor, motion.token.owner)
                        .map_err(|_| CombatRejection::Busy)?;
                }
                self.stop_physical_approach(world, actor);
                self.cancel(actor);
                self.missiles
                    .retain(|_, m| m.proposal.actor != actor.0 || m.launched);
                Ok(CombatChange::Cancelled)
            }
            CombatRequest::QueryHealth(target) => {
                let target_state = world
                    .combatant(target)
                    .ok_or(CombatRejection::MissingCombatProfile)?;
                // Queries require an authoritative nearby target; not a global ID oracle.
                let (nearby, clear) = world
                    .attack_geometry(actor, target, 192.0)
                    .map_err(|_| CombatRejection::MissingActor)?;
                if !nearby {
                    return Err(CombatRejection::OutOfRange);
                }
                if !clear {
                    return Err(CombatRejection::Obstructed);
                }
                Ok(CombatChange::Health {
                    target,
                    current: target_state.health(),
                    maximum: target_state.profile().maximum_health,
                })
            }
            CombatRequest::TargetedMelee {
                target,
                height,
                power,
            } => {
                if target == actor
                    || !(1..=3).contains(&height)
                    || !power.is_finite()
                    || !(0.0..=1.0).contains(&power)
                {
                    return Err(CombatRejection::InvalidRequest);
                }
                if source.mode() != 2 {
                    return Err(CombatRejection::UnsupportedMode);
                }
                if self.attacks.contains_key(&actor) {
                    return Err(CombatRejection::Busy);
                }
                if self.attacks.len() >= self.capacity {
                    return Err(CombatRejection::Capacity);
                }
                let target_state = world
                    .combatant(target)
                    .ok_or(CombatRejection::MissingCombatProfile)?;
                if target_state.health() == 0 {
                    return Err(CombatRejection::Dead);
                }
                if self.physical.contains_key(&actor) {
                    return self.start_physical(world, actor, target, height, power, now);
                }
                if source.profile().player && target_state.profile().player {
                    return Err(CombatRejection::InvalidRequest);
                }
                let profile = source.profile();
                let (in_range, clear) = world
                    .attack_geometry(actor, target, profile.melee_range)
                    .map_err(|_| CombatRejection::MissingActor)?;
                if !in_range {
                    return Err(CombatRejection::OutOfRange);
                }
                if !clear {
                    return Err(CombatRejection::Obstructed);
                }
                let hooks: Vec<_> = profile
                    .strike_offsets
                    .iter()
                    .map(|t| Strike { offset_seconds: *t })
                    .collect();
                let attack = MeleeAttack::new(target.0, now, profile.attack_duration, &hooks)
                    .map_err(|_| CombatRejection::InvalidRequest)?;
                let power_modifier = if source.profile().player {
                    power + 0.5
                } else {
                    1.0
                };
                let random = self.next_random()?;
                if let Some(skills) = self.skills.get_mut(&actor) {
                    skills.power = power;
                }
                self.attacks.insert(
                    actor,
                    ActiveAttack {
                        credited_owner: world.damage_owner(actor),
                        attack,
                        power_modifier,
                        power,
                        height,
                        random,
                    },
                );
                Ok(CombatChange::AttackStarted { target })
            }
        }
    }
    /// One result per actor per tick; full outbox retains pending attacks. Bodies
    /// still tick independently. Damage uses prepared fixture inputs here; full
    /// weapon/body-part/armor/critical calculation is a separate source-backed port.
    #[cfg(test)]
    fn step_internal(
        &mut self,
        world: &mut World,
        now: f64,
        inventory: Option<&crate::inventory::Inventory>,
    ) {
        self.step_internal_observed(world, now, inventory, None);
    }
    fn step_internal_observed(
        &mut self,
        world: &mut World,
        now: f64,
        inventory: Option<&crate::inventory::Inventory>,
        characters: Option<&crate::characters::Characters>,
    ) {
        self.simulation_now = now;
        self.physical_deadlines.retain(|_, until| now < *until);
        self.drain_physical_motion(world);
        self.step_physical(world, now, inventory, false);
        self.step_missiles(world, now, inventory, false);
        self.cleanup_physical_faults(world);
        self.adopt_native_missile_launches(world, inventory, characters);
        self.step_physical_drivers(world, now, inventory);
        let mut completed = None;
        for (&actor, attack) in &mut self.attacks {
            if self.events.len() == self.capacity || self.dirty.len() == self.capacity {
                break;
            }
            let target = EntityId(attack.attack.target());
            if world.validate_health_observations([target]).is_err() {
                continue;
            }
            if [actor, target].into_iter().any(|id| {
                world.vital_reserved(id, bace_entity::EntityVital::Health)
                    || world.vital_reserved(id, bace_entity::EntityVital::Stamina)
            }) {
                continue;
            }
            let source = world.combatant(actor);
            let defender = world.combatant(target);
            let attacker_alive = source.is_some_and(|s| s.health() != 0);
            let target_alive = defender.is_some_and(|s| s.health() != 0);
            let range = source.map_or(0.0, |s| s.profile().melee_range);
            let base_damage = source.map_or(0, |s| s.profile().melee_damage);
            let attacker_player = source.is_some_and(|s| s.profile().player);
            let defender_player = defender.is_some_and(|s| s.profile().player);
            let defender_in_combat = defender.is_some_and(|s| s.mode() != 1);
            let defender_reckless_mode = defender.is_some_and(|s| matches!(s.mode(), 2 | 4));
            let (in_range, clear) = world
                .attack_geometry(actor, target, range)
                .unwrap_or((false, false));
            match attack
                .attack
                .poll(now, attacker_alive, target_alive, in_range, clear)
            {
                Ok(AttackImpact::Strike { index }) => {
                    let effect = accepted_angle(world, actor, target).and_then(|angle| {
                        damage_and_dirty(ImpactInputs {
                            attacker: self.skills.get(&actor),
                            defender: self.skills.get(&target),
                            attacker_player,
                            defender_player,
                            defender_in_combat,
                            defender_reckless_mode,
                            angle,
                            power: attack.power,
                            height: attack.height,
                            base_damage,
                            power_modifier: attack.power_modifier,
                            random: attack.random.as_ref(),
                            index,
                        })
                    });
                    let (damage, dirty) = match effect {
                        Ok(v) => v,
                        Err(_) => {
                            attack.attack.cancel();
                            self.events.push_back(CombatEvent::Finished {
                                actor,
                                actor_incarnation: incarnation(world, actor),
                                cancelled: true,
                            });
                            completed = Some(actor);
                            break;
                        }
                    };
                    match world.damage_from(target, attack.credited_owner, damage) {
                        Ok(amount) => {
                            let state = world.combatant(target).expect("accepted damage");
                            if dirty.count > 0 {
                                self.dirty.push_back(DirtyFightingImpact {
                                    attacker: actor,
                                    target,
                                    spells: dirty.spells,
                                    count: dirty.count,
                                });
                            }
                            self.events.push_back(CombatEvent::Damage {
                                target_incarnation: state.incarnation(),
                                attacker: Some(actor),
                                death_blow: None,
                                target,
                                amount,
                                current: state.health(),
                                maximum: state.profile().maximum_health,
                                killed: state.health() == 0,
                                revision: state.revision(),
                            });
                        }
                        Err(_) => {
                            attack.attack.cancel();
                            self.events.push_back(CombatEvent::Finished {
                                actor,
                                actor_incarnation: incarnation(world, actor),
                                cancelled: true,
                            });
                            completed = Some(actor);
                            break;
                        }
                    }
                }
                Ok(AttackImpact::Waiting) => {}
                result => {
                    self.events.push_back(CombatEvent::Finished {
                        actor,
                        actor_incarnation: incarnation(world, actor),
                        cancelled: !matches!(result, Ok(AttackImpact::Complete)),
                    });
                    completed = Some(actor);
                    break;
                }
            }
        }
        // Avoid an allocation per tick for removal bookkeeping. One terminal
        // attack is retired per tick; retained records remain bounded.
        if let Some(actor) = completed {
            self.attacks.remove(&actor);
        }
    }
}

fn incarnation(world: &World, actor: EntityId) -> u64 {
    world
        .combatant(actor)
        .map_or(0, |state| state.incarnation())
}
