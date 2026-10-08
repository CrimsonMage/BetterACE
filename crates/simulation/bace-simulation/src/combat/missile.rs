use super::*;
use bace_combat::physical::{
    PhysicalDamageInput, SelectedPhysicalAttack, missile_velocity, resolve_physical,
};
use bace_gameplay_api::weapon_combat::*;
use bace_geometry::Vec3;
use bace_physics::{ProjectileBody, ProjectileStep};
use bace_world::OwnedProjectile;
use std::sync::Arc;
pub(crate) struct MissileAttackState {
    appearance: Option<Arc<bace_content::WeenieV1>>,
    pub(super) motion: Option<super::motion::PhysicalMotion>,
    lifetime: Option<bace_combat::physical::MissileLifetime>,
    remove_pending: bool,
    pub(super) pending_contact: Option<ProjectileStep>,
    pub proposal: PhysicalLaunchProposal,
    pub profile: Arc<PhysicalCombatProfile>,
    pub selected: SelectedPhysicalAttack,
    pub height: u32,
    pub power: f32,
    pub random: bace_random::RandomStream,
    pub due: f64,
    pub submitted: bool,
    pub launched: bool,
    pub faulted: bool,
}
impl Combat {
    pub(crate) fn references_region(&self, block: u16, world: &World) -> bool {
        self.missiles.values().any(|state| {
            state
                .proposal
                .flight
                .as_ref()
                .is_some_and(|f| f.cell >> 16 == u32::from(block))
                || world
                    .projectile(EntityId(state.proposal.projectile))
                    .is_some_and(|p| p.cell.0 >> 16 == u32::from(block))
        })
    }

    pub(crate) fn reserves_projectile(&self, id: EntityId) -> bool {
        self.missile_ids.contains(&id)
            || self
                .missiles
                .values()
                .any(|v| v.proposal.projectile == id.0)
    }
    pub(crate) fn supply_missile_id(
        &mut self,
        id: EntityId,
        world: &World,
    ) -> Result<(), CombatRejection> {
        if id.0 == 0 || world.contains_identity(id) || self.reserves_projectile(id) {
            return Err(CombatRejection::InvalidRequest);
        }
        if self.missile_ids.len() >= self.capacity {
            return Err(CombatRejection::Capacity);
        }
        self.missile_ids.push_back(id);
        Ok(())
    }
    pub(crate) fn pending_launch(&self, operation: u64) -> Option<&PhysicalLaunchProposal> {
        self.missiles
            .get(&operation)
            .filter(|s| s.submitted && !s.launched)
            .map(|s| &s.proposal)
    }
    pub(crate) fn pending_launch_appearance(
        &self,
        operation: u64,
    ) -> Option<Arc<bace_content::WeenieV1>> {
        self.missiles
            .get(&operation)
            .filter(|state| state.submitted && !state.launched)
            .and_then(|state| state.appearance.clone())
    }
    /// Definite durable rejection only; uncertain commits retain this owner.
    pub(crate) fn reject_launch(
        &mut self,
        operation: u64,
        world: &mut World,
    ) -> Result<(), CombatRejection> {
        let state = self
            .missiles
            .get(&operation)
            .filter(|s| s.submitted && !s.launched)
            .ok_or(CombatRejection::InvalidRequest)?;
        let actor = EntityId(state.proposal.actor);
        if let Some(motion) = &state.motion {
            super::faults::end_if_owned(world, actor, motion.token.owner)
                .map_err(|_| CombatRejection::Busy)?;
        }
        self.missiles.remove(&operation);
        self.launches.retain(|p| p.operation != operation);
        self.cancel(actor);
        Ok(())
    }
    pub(crate) fn take_launch(&mut self) -> Option<PhysicalLaunchProposal> {
        self.launches.pop_front()
    }
    pub(crate) fn retry_launch(&mut self, operation: u64) -> Result<(), CombatRejection> {
        let state = self
            .missiles
            .get_mut(&operation)
            .ok_or(CombatRejection::InvalidRequest)?;
        if state.launched {
            return Err(CombatRejection::InvalidRequest);
        }
        if self.launches.len() >= self.capacity {
            return Err(CombatRejection::Capacity);
        }
        if !self.launches.iter().any(|p| p.operation == operation) {
            self.launches.push_back(state.proposal.clone());
        }
        Ok(())
    }
    pub(super) fn start_missile(
        &mut self,
        world: &mut World,
        actor: EntityId,
        target: EntityId,
        height: u32,
        accuracy: f32,
        now: f64,
    ) -> Result<CombatChange, CombatRejection> {
        if actor == target
            || !(1..=3).contains(&height)
            || !accuracy.is_finite()
            || !(0.0..=1.0).contains(&accuracy)
        {
            return Err(CombatRejection::InvalidRequest);
        }
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
                .any(|s| s.proposal.actor == actor.0 && !s.launched)
        {
            return Err(CombatRejection::Busy);
        }
        if self.missiles.len() >= self.capacity {
            return Err(CombatRejection::Capacity);
        }
        let profile = self
            .physical
            .get(&actor)
            .ok_or(CombatRejection::MissingCombatProfile)?
            .clone();
        let defender = self
            .physical
            .get(&target)
            .ok_or(CombatRejection::MissingCombatProfile)?;
        if !super::physical::runtime_permission(world, actor, target, &profile, defender) {
            return Err(CombatRejection::InvalidRequest);
        }
        if world.combatant(actor).is_none_or(|s| s.mode() != 4) {
            return Err(CombatRejection::UnsupportedMode);
        }
        if world.combatant(target).is_none_or(|s| s.health() == 0) {
            return Err(CombatRejection::Dead);
        }
        if !matches!(world.attack_geometry(actor, target, 80.0), Ok((true, _))) {
            return Err(CombatRejection::OutOfRange);
        }
        let spec = profile
            .missile
            .ok_or(CombatRejection::MissingCombatProfile)?;
        let launcher = profile
            .launcher
            .as_ref()
            .ok_or(CombatRejection::MissingCombatProfile)?;
        let ammo = profile
            .ammunition
            .as_ref()
            .or(Some(launcher))
            .ok_or(CombatRejection::MissingCombatProfile)?;
        if spec.ammunition_count == 0 {
            return Err(CombatRejection::InvalidRequest);
        }
        let speed = if self.physical_drivers.contains_key(&actor) {
            bace_combat::physical::missile_attack_speed(profile.quickness, launcher.attack_time)
                .map_err(|_| CombatRejection::InvalidRequest)?
        } else {
            1.0
        };
        let deadline = now
            + self
                .physical_chain(world, actor, spec.attack_motion, speed)
                .map_or(spec.duration_seconds, |c| c.nominal_duration_seconds());
        let due = now + spec.launch_seconds;
        if !deadline.is_finite() || deadline <= now || !due.is_finite() {
            return Err(CombatRejection::InvalidRequest);
        }
        let skill = profile
            .skills
            .iter()
            .find(|s| s.0 == launcher.skill)
            .ok_or(CombatRejection::MissingCombatProfile)?
            .1
            .current;
        let level = skill;
        let operation = self
            .next_attack
            .checked_add(1)
            .ok_or(CombatRejection::Capacity)?;
        let (root, epoch) = self
            .random
            .as_ref()
            .ok_or(CombatRejection::InvalidRequest)?;
        let mut id = [0; 16];
        id[..8].copy_from_slice(&epoch.to_le_bytes());
        id[8..].copy_from_slice(&operation.to_le_bytes());
        let random = root
            .event_stream(id, bace_random::Domain::Combat)
            .map_err(|_| CombatRejection::InvalidRequest)?;
        let projectile = self
            .missile_ids
            .front()
            .copied()
            .ok_or(CombatRejection::Capacity)?;
        let appearance = self
            .physical_sources
            .get(&actor)
            .and_then(|source| {
                source
                    .equipment
                    .iter()
                    .find(|item| item.entity == ammo.entity && item.revision == ammo.revision)
            })
            .map(|item| item.weenie.clone());
        if appearance.is_none()
            && world
                .body(actor)
                .is_ok_and(|body| body.collision_shape().is_some())
        {
            return Err(CombatRejection::MissingCombatProfile);
        }
        let motion =
            self.begin_physical_motion(world, actor, operation, spec.attack_motion, speed)?;
        self.missile_ids.pop_front();
        let proposal = PhysicalLaunchProposal {
            operation,
            flight: None,
            event_id: id,
            actor: actor.0,
            credited_owner: world.damage_owner(actor).0,
            target: target.0,
            projectile: projectile.0,
            ammunition: ammo.entity,
            ammunition_revision: ammo.revision,
            expected_count: spec.ammunition_count,
            consumed: u32::from(profile.player),
            profile_revision: profile.revision,
            content_hash: profile.content_hash,
        };
        self.next_attack = operation;
        self.physical_deadlines.insert(actor, deadline);
        self.missiles.insert(
            operation,
            MissileAttackState {
                appearance,
                motion,
                lifetime: None,
                remove_pending: false,
                pending_contact: None,
                proposal,
                selected: SelectedPhysicalAttack {
                    hand: PhysicalHand::Main,
                    maneuver: 0,
                    attack_type: 0,
                    skill: launcher.skill,
                    skill_level: level,
                    speed,
                },
                profile,
                height,
                power: accuracy,
                random,
                due,
                submitted: false,
                launched: false,
                faulted: false,
            },
        );
        Ok(CombatChange::AttackStarted { target })
    }
    /// NPC ammunition is infinite in pinned Creature_Missile. Launches consume
    /// no valuable item state and can adopt at the same owner observation point.
    pub(super) fn adopt_native_missile_launches(
        &mut self,
        world: &mut World,
        inventory: Option<&crate::inventory::Inventory>,
        characters: Option<&crate::characters::Characters>,
    ) {
        let count = self.launches.len();
        for _ in 0..count {
            let Some(proposal) = self.launches.pop_front() else {
                break;
            };
            let native = self
                .missiles
                .get(&proposal.operation)
                .is_some_and(|m| !m.profile.player && m.proposal.consumed == 0);
            let reserved = inventory.is_some_and(|i| {
                i.reserved(EntityId(proposal.actor)) || i.reserved(EntityId(proposal.ammunition))
            });
            if !native || reserved {
                self.launches.push_back(proposal);
                continue;
            }
            let receipt = PhysicalLaunchReceipt {
                operation: proposal.operation,
                ammunition: proposal.ammunition,
                before_revision: proposal.ammunition_revision,
                after_revision: proposal.ammunition_revision,
                remaining: proposal.expected_count,
            };
            if self
                .confirm_launch_internal(receipt, world, characters)
                .is_err()
            {
                self.launches.push_back(proposal);
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn confirm_launch(
        &mut self,
        receipt: PhysicalLaunchReceipt,
        world: &mut World,
    ) -> Result<(), CombatRejection> {
        self.confirm_launch_internal(receipt, world, None)
    }
    pub(crate) fn confirm_launch_with_observers(
        &mut self,
        receipt: PhysicalLaunchReceipt,
        world: &mut World,
        characters: &crate::characters::Characters,
    ) -> Result<(), CombatRejection> {
        self.confirm_launch_internal(receipt, world, Some(characters))
    }
    fn confirm_launch_internal(
        &mut self,
        receipt: PhysicalLaunchReceipt,
        world: &mut World,
        characters: Option<&crate::characters::Characters>,
    ) -> Result<(), CombatRejection> {
        let actor = self
            .missiles
            .get(&receipt.operation)
            .map(|s| EntityId(s.proposal.actor))
            .ok_or(CombatRejection::InvalidRequest)?;
        let auto_peace = receipt.remaining == 0
            && world
                .combatant(actor)
                .is_some_and(|s| s.health() > 0 && s.mode() == 4);
        let peace = if auto_peace {
            self.prepare_mode_style(world, actor, 1)?
        } else {
            None
        };
        if self.physical_events.len() + 1 + usize::from(auto_peace) > self.capacity.max(2) {
            return Err(CombatRejection::Capacity);
        }
        let state = self
            .missiles
            .get_mut(&receipt.operation)
            .ok_or(CombatRejection::InvalidRequest)?;
        let p = &state.proposal;
        if !state.submitted
            || state.launched
            || receipt.ammunition != p.ammunition
            || receipt.before_revision != p.ammunition_revision
            || receipt.after_revision
                != receipt
                    .before_revision
                    .checked_add(u64::from(p.consumed))
                    .ok_or(CombatRejection::Capacity)?
            || receipt.remaining != p.expected_count - p.consumed
        {
            return Err(CombatRejection::InvalidRequest);
        }
        let actor = EntityId(p.actor);
        let target = EntityId(p.target);
        if self.physical_sources.contains_key(&actor) && i32::try_from(receipt.remaining).is_err() {
            return Err(CombatRejection::InvalidRequest);
        }
        let flight = p.flight.as_ref().ok_or(CombatRejection::InvalidRequest)?;
        let body = ProjectileBody::new(
            Vec3::new(flight.origin[0], flight.origin[1], flight.origin[2]),
            Vec3::new(flight.velocity[0], flight.velocity[1], flight.velocity[2]),
            flight.radius,
            flight.gravity,
            flight.lifetime,
        )
        .map_err(|_| CombatRejection::InvalidRequest)?;
        let cell = bace_types::CellId(flight.cell);
        let next_revision = state
            .profile
            .revision
            .checked_add(1)
            .ok_or(CombatRejection::Capacity)?;
        world
            .insert_projectile(
                EntityId(p.projectile),
                OwnedProjectile {
                    cell,
                    source: actor,
                    target: Some(target),
                    body,
                },
            )
            .map_err(|_| CombatRejection::Capacity)?;
        let launch = match world.projectile_launch_snapshot(
            EntityId(p.projectile),
            self.simulation_tick,
            |id| characters.and_then(|c| c.entered_binding(id)),
        ) {
            Ok(launch) => Arc::new(launch),
            Err(_) => {
                world.remove_projectile(EntityId(p.projectile));
                return Err(CombatRejection::MissingCombatProfile);
            }
        };
        self.physical_events
            .push_back(PhysicalCombatEvent::ProjectileCreated {
                proposal: p.clone(),
                tick: self.simulation_tick,
                launch,
                appearance: state.appearance.clone(),
            });
        state.launched = true;
        state.lifetime = Some(
            bace_combat::physical::MissileLifetime::launch(self.simulation_now)
                .expect("explicit owner clock"),
        );
        let mut profile = (*state.profile).clone();
        profile.revision = next_revision;
        if let Some(spec) = &mut profile.missile {
            spec.ammunition_count = receipt.remaining;
        }
        // This derived next-attack roster follows the same committed ammo change.
        // The in-flight state retains its original immutable firing profile.
        if receipt.remaining == 0 {
            profile
                .equipment
                .retain(|stamp| stamp.entity != receipt.ammunition);
        } else if let Some(stamp) = profile
            .equipment
            .iter_mut()
            .find(|stamp| stamp.entity == receipt.ammunition)
        {
            stamp.revision = receipt.after_revision;
        }
        if let Some(ammo) = &mut profile.ammunition {
            ammo.revision = receipt.after_revision;
        } else if let Some(launcher) = &mut profile.launcher {
            launcher.revision = receipt.after_revision;
        }
        let profile = Arc::new(profile);
        self.physical.insert(actor, profile.clone());
        self.adopt_physical_ammunition_source(actor, receipt, profile);
        if auto_peace {
            let state = world.combatant_mut(actor).expect("preflight actor");
            state.set_mode(1);
            let actor_incarnation = state.incarnation();
            self.queue_automatic_mode_style(actor, peace);
            self.physical_events
                .push_back(PhysicalCombatEvent::ModeChanged {
                    actor,
                    actor_incarnation,
                    mode: 1,
                });
        }
        Ok(())
    }
    pub(super) fn step_missiles(
        &mut self,
        world: &mut World,
        now: f64,
        inventory: Option<&crate::inventory::Inventory>,
        only_pending: bool,
    ) {
        let mut finished = None;
        for (&operation, state) in &mut self.missiles {
            if only_pending
                && !self
                    .physical_hits
                    .keys()
                    .any(|key| key.kind == PhysicalKind::Missile && key.operation == operation)
            {
                continue;
            }
            if state.faulted {
                continue;
            }
            if !state.submitted {
                if inventory.is_some_and(|i| {
                    !super::equipment::equipment_current(
                        &state.profile,
                        EntityId(state.proposal.actor),
                        i,
                    )
                }) {
                    if self.physical_events.len() == self.capacity.max(2) {
                        return;
                    }
                    self.physical_events.push_back(PhysicalCombatEvent::Fault {
                        actor: EntityId(state.proposal.actor),
                        actor_incarnation: incarnation(world, EntityId(state.proposal.actor)),
                        target: EntityId(state.proposal.target),
                        operation,
                        error: CombatRejection::MissingCombatProfile,
                    });
                    state.faulted = true;
                    continue;
                }
                if state
                    .motion
                    .as_ref()
                    .is_some_and(|m| m.complete == Some(false))
                {
                    if self.physical_events.len() == self.capacity.max(2) {
                        return;
                    }
                    self.physical_events.push_back(PhysicalCombatEvent::Fault {
                        actor: EntityId(state.proposal.actor),
                        actor_incarnation: incarnation(world, EntityId(state.proposal.actor)),
                        target: EntityId(state.proposal.target),
                        operation,
                        error: CombatRejection::InvalidRequest,
                    });
                    state.faulted = true;
                    continue;
                }
                if state
                    .motion
                    .as_ref()
                    .map_or(now < state.due, |m| m.complete != Some(true))
                {
                    continue;
                }
                if self.launches.len() >= self.capacity {
                    break;
                }
                if let Some(m) = &state.motion
                    && super::faults::end_if_owned(
                        world,
                        EntityId(state.proposal.actor),
                        m.token.owner,
                    )
                    .is_err()
                {
                    continue;
                }
                match prepare_flight(
                    world,
                    state,
                    &self.physical,
                    self.physical_options
                        .get(&EntityId(state.proposal.actor))
                        .map(|v| v.1),
                ) {
                    Ok(flight) => state.proposal.flight = Some(flight),
                    Err(error) => {
                        if self.physical_events.len() == self.capacity.max(2) {
                            break;
                        }
                        self.physical_events.push_back(
                            super::physical::PhysicalCombatEvent::Fault {
                                actor: EntityId(state.proposal.actor),
                                actor_incarnation: incarnation(
                                    world,
                                    EntityId(state.proposal.actor),
                                ),
                                target: EntityId(state.proposal.target),
                                operation,
                                error,
                            },
                        );
                        state.faulted = true;
                        continue;
                    }
                }
                self.launches.push_back(state.proposal.clone());
                state.submitted = true;
                continue;
            }
            if !state.launched {
                continue;
            }
            if self.events.len() >= self.capacity
                || self.physical_events.len() >= self.capacity.max(2)
                || self.dirty.len() >= self.capacity
            {
                break;
            }
            let id = EntityId(state.proposal.projectile);
            let actor = EntityId(state.proposal.actor);
            if state.remove_pending {
                world.remove_projectile(id);
                self.physical_events
                    .push_back(PhysicalCombatEvent::ProjectileRemoved {
                        actor: id,
                        tick: self.simulation_tick,
                    });
                finished = Some(operation);
                break;
            }
            let lifetime = state
                .lifetime
                .as_mut()
                .expect("receipt installed launch lifetime");
            match if state.pending_contact.is_some() {
                bace_combat::physical::MissileLifetimeEvent::Waiting
            } else {
                lifetime.poll(now).expect("validated simulation time")
            } {
                bace_combat::physical::MissileLifetimeEvent::DestroyEffect => {
                    self.physical_events
                        .push_back(PhysicalCombatEvent::ProjectileDestroy {
                            actor: id,
                            tick: self.simulation_tick,
                        });
                    continue;
                }
                bace_combat::physical::MissileLifetimeEvent::Remove => {
                    world.remove_projectile(id);
                    self.physical_events
                        .push_back(PhysicalCombatEvent::ProjectileRemoved {
                            actor: id,
                            tick: self.simulation_tick,
                        });
                    finished = Some(operation);
                    break;
                }
                bace_combat::physical::MissileLifetimeEvent::Waiting => {}
            }
            if lifetime.stopped() && state.pending_contact.is_none() {
                continue;
            }
            let contact = if let Some(contact) = state.pending_contact {
                Ok(contact)
            } else {
                world.step_projectile(id, 1.0 / 30.0)
            };
            match contact {
                Ok(ProjectileStep::Flying) => {}
                Ok(ProjectileStep::Impact {
                    target: Some(target),
                }) => {
                    state.pending_contact = Some(ProjectileStep::Impact {
                        target: Some(target),
                    });
                    let target = EntityId(target);
                    if world.validate_health_observations([target]).is_err()
                        || world.vital_reserved(target, bace_entity::EntityVital::Health)
                        || world.vital_reserved(target, bace_entity::EntityVital::Stamina)
                    {
                        continue;
                    }
                    let (attacker_name, target_name) = match (
                        super::physical::capture_combat_name(world, actor),
                        super::physical::capture_combat_name(world, target),
                    ) {
                        (Ok(source), Ok(target)) => (source, target),
                        _ => {
                            self.physical_events.push_back(
                                super::physical::PhysicalCombatEvent::Fault {
                                    actor,
                                    actor_incarnation: incarnation(world, actor),
                                    target,
                                    operation,
                                    error: CombatRejection::InvalidRequest,
                                },
                            );
                            state.remove_pending = true;
                            state.pending_contact = None;
                            break;
                        }
                    };
                    if let Some(defender) = self.physical.get(&target)
                        && inventory.is_none_or(|i| {
                            super::equipment::equipment_current(defender, target, i)
                        })
                        && let Some(vital) = world.combatant(target)
                        && let Ok(rolls) = super::physical::impact_rolls(&state.random, 0, target)
                        && vital.can_record_damage(EntityId(state.proposal.credited_owner))
                        && vital.revision() != u64::MAX
                    {
                        let input = PhysicalDamageInput {
                            attacker: &state.profile,
                            pk_override: Some((
                                super::physical::live_pk(world, actor, state.profile.pk),
                                super::physical::live_pk(world, target, defender.pk),
                            )),
                            rating_override: Some((
                                self.physical_ratings
                                    .get(&actor)
                                    .copied()
                                    .unwrap_or(state.profile.ratings),
                                self.physical_ratings
                                    .get(&target)
                                    .copied()
                                    .unwrap_or(defender.ratings),
                            )),
                            defender,
                            selected: state.selected,
                            kind: PhysicalKind::Missile,
                            hook_part: 0,
                            height: state.height,
                            original_power: state.power,
                            power: state.power,
                            target_angle_degrees: f64::from(
                                accepted_angle(world, actor, target).unwrap_or(0.0),
                            ),
                            defender_stamina: vital
                                .vital(bace_entity::EntityVital::Stamina)
                                .map_or(0, |v| v.current),
                            defender_in_combat: vital.mode() != 1,
                            rolls,
                        };
                        let key = bace_gameplay_api::physical_procs::PhysicalHitKey {
                            attacker: actor,
                            operation,
                            ordinal: 0,
                            target,
                            kind: PhysicalKind::Missile,
                        };
                        let impact = if self.physical_procs_enabled {
                            let draws = match super::procs::proc_draws(&state.random, 0, target) {
                                Ok(draws) => draws,
                                Err(error) => {
                                    self.physical_events.push_back(PhysicalCombatEvent::Fault {
                                        actor,
                                        actor_incarnation: incarnation(world, actor),
                                        target,
                                        operation,
                                        error,
                                    });
                                    state.faulted = true;
                                    continue;
                                }
                            };
                            let existed = self.physical_hits.contains_key(&key);
                            let progressed = super::procs::advance_hit(
                                &mut self.physical_hits,
                                self.capacity,
                                key,
                                input,
                                state.profile.launcher.as_ref().map(|w| EntityId(w.entity)),
                                draws,
                            );
                            if !existed
                                && super::procs::contact_evaded(&self.physical_hits, key)
                                    == Some(false)
                            {
                                crate::pk_activity::record_pair(world, actor, target, now);
                            }
                            match progressed {
                                Ok(Some(value)) => value,
                                Ok(None) => continue,
                                Err(error) => {
                                    self.physical_events.push_back(PhysicalCombatEvent::Fault {
                                        actor,
                                        actor_incarnation: incarnation(world, actor),
                                        target,
                                        operation,
                                        error,
                                    });
                                    state.faulted = true;
                                    continue;
                                }
                            }
                        } else {
                            match resolve_physical(input) {
                                Ok(value) => value,
                                Err(_) => {
                                    state.remove_pending = true;
                                    state.pending_contact = None;
                                    continue;
                                }
                            }
                        };
                        if !self.physical_procs_enabled && !impact.evaded {
                            crate::pk_activity::record_pair(world, actor, target, now);
                        }
                        let amount = world
                            .damage_from(
                                target,
                                EntityId(state.proposal.credited_owner),
                                impact.damage,
                            )
                            .expect("preflight");
                        let vital = world.combatant(target).expect("accepted damage");
                        if !self.physical_procs_enabled && impact.dirty_count > 0 {
                            self.dirty.push_back(DirtyFightingImpact {
                                attacker: actor,
                                target,
                                spells: impact.dirty_spells,
                                count: impact.dirty_count,
                            });
                        }
                        self.physical_events.push_back(
                            super::physical::PhysicalCombatEvent::Impact {
                                attacker: actor,
                                attacker_incarnation: incarnation(world, actor),
                                target_incarnation: vital.incarnation(),
                                target,
                                impact,
                                attacker_name,
                                target_name,
                                applied: amount,
                                current: vital.health(),
                                maximum: vital.profile().maximum_health,
                                revision: vital.revision(),
                            },
                        );
                        self.events.push_back(CombatEvent::Damage {
                            target_incarnation: vital.incarnation(),
                            attacker: Some(actor),
                            death_blow: None,
                            target,
                            amount,
                            current: vital.health(),
                            maximum: vital.profile().maximum_health,
                            killed: amount > 0 && vital.health() == 0,
                            revision: vital.revision(),
                        });
                    }
                    self.physical_hits
                        .retain(|key, _| !(key.attacker == actor && key.operation == operation));
                    state.remove_pending = true;
                    state.pending_contact = None;
                    break;
                }
                Err(_) => {
                    self.physical_events
                        .push_back(super::physical::PhysicalCombatEvent::Fault {
                            actor,
                            actor_incarnation: incarnation(world, actor),
                            target: EntityId(state.proposal.target),
                            operation,
                            error: CombatRejection::Obstructed,
                        });
                    state.faulted = true;
                    break;
                }
                Ok(ProjectileStep::Impact { target: None }) if state.profile.player => {
                    state
                        .lifetime
                        .as_mut()
                        .expect("launched lifetime")
                        .environment(now)
                        .expect("validated time");
                    self.physical_events
                        .push_back(PhysicalCombatEvent::ProjectileResting {
                            actor: id,
                            tick: self.simulation_tick,
                        });
                }
                _ => {
                    world.remove_projectile(id);
                    self.physical_events
                        .push_back(PhysicalCombatEvent::ProjectileRemoved {
                            actor: id,
                            tick: self.simulation_tick,
                        });
                    finished = Some(operation);
                    break;
                }
            }
        }
        if let Some(id) = finished {
            self.missiles.remove(&id);
        }
    }
}

fn prepare_flight(
    world: &World,
    state: &MissileAttackState,
    profiles: &BTreeMap<EntityId, Arc<PhysicalCombatProfile>>,
    options2: Option<u32>,
) -> Result<PhysicalFlight, CombatRejection> {
    let actor = EntityId(state.proposal.actor);
    let target = EntityId(state.proposal.target);
    let spec = state
        .profile
        .missile
        .ok_or(CombatRejection::MissingCombatProfile)?;
    let (cell, source) = world
        .actor_state(actor)
        .map_err(|_| CombatRejection::MissingActor)?;
    let (target_cell, dest) = world
        .actor_state(target)
        .map_err(|_| CombatRejection::MissingActor)?;
    if cell != target_cell {
        return Err(CombatRejection::Obstructed);
    }
    let defender = profiles
        .get(&target)
        .ok_or(CombatRejection::MissingCombatProfile)?;
    if !super::physical::runtime_permission(world, actor, target, &state.profile, defender) {
        return Err(CombatRejection::InvalidRequest);
    }
    let target_pos = dest.position()
        + Vec3::new(
            0.0,
            0.0,
            defender.height
                * match state.height {
                    1 => 5.0 / 6.0,
                    3 => 1.0 / 6.0,
                    _ => 0.5,
                },
        );
    let mut from = source.position() + Vec3::new(0.0, 0.0, state.profile.height * 0.75);
    let delta = target_pos - from;
    let len = delta.length_squared().sqrt();
    if len <= 0.0002 {
        return Err(CombatRejection::InvalidRequest);
    }
    from = from
        + delta
            * (1.0 / len)
            * (world
                .body(actor)
                .map_err(|_| CombatRejection::MissingActor)?
                .collision_radius()
                + spec.radius
                + 0.1);
    if !world
        .scene(cell)
        .map_err(|_| CombatRejection::Obstructed)?
        .valid_placement(from, spec.radius)
    {
        return Err(CombatRejection::Obstructed);
    }
    let offset = target_pos - from;
    let tv = dest.velocity();
    let (speed, tracking) =
        if let Some(options) = options2.or_else(|| state.motion.as_ref().map(|_| 0)) {
            bace_combat::physical::missile_settings(spec.speed, state.profile.player, options)
                .map_err(|_| CombatRejection::InvalidRequest)?
        } else {
            (spec.speed, spec.tracking)
        };
    let v = missile_velocity(
        [offset.x, offset.y, offset.z],
        [tv.x, tv.y, tv.z],
        tracking,
        spec.gravity,
        speed,
    )
    .map_err(|_| CombatRejection::InvalidRequest)?;
    let gravity = if spec.gravity { -9.8 } else { 0.0 };
    ProjectileBody::new(from, Vec3::new(v[0], v[1], v[2]), spec.radius, gravity, 7.0)
        .map_err(|_| CombatRejection::InvalidRequest)?;
    Ok(PhysicalFlight {
        cell: cell.0,
        origin: [from.x, from.y, from.z],
        velocity: v,
        radius: spec.radius,
        gravity,
        lifetime: 7.0,
    })
}
