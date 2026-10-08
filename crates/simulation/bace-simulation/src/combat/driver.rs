//! One physical request owner and one replaceable queued request per actor.
//! GDLE AttackManager supplies repeat ownership; client build overlap determines
//! the retained manual charge origin. All timestamps are simulation supplied.
use super::*;
use bace_combat::physical::charge_seconds;

pub(crate) struct AttackDriver {
    current: Option<CombatRequest>,
    pub(super) mode_style: Option<u32>,
    queued: Option<CombatRequest>,
    due: f64,
    started: Option<f64>,
    operation: u64,
    timeout: f64,
    controller: Option<bace_motion::TurnControl>,
    pub(super) reload: Option<super::motion::PhysicalMotion>,
    pub(super) transition: Option<super::motion::PhysicalMotion>,
    needs_reload: bool,
    reload_speed: f32,
}
impl AttackDriver {
    pub(super) fn cancel_operation(&mut self, operation: u64) {
        if self.operation == operation {
            self.cancel();
        }
    }
    pub(super) fn cancel(&mut self) {
        self.current = None;
        self.queued = None;
        self.mode_style = None;
        self.needs_reload = false;
    }
    pub(super) fn active(&self) -> bool {
        self.current.is_some()
            || self.mode_style.is_some()
            || self.queued.is_some()
            || self.controller.is_some()
            || self.reload.is_some()
            || self.transition.is_some()
            || self.needs_reload
    }
}
impl Combat {
    pub(crate) fn has_physical_driver(&self, actor: EntityId) -> bool {
        self.physical_drivers.contains_key(&actor)
    }
    pub(crate) fn capture_physical_recovery(&self, actor: EntityId, now: f64) -> f64 {
        let deadline = self.physical_deadlines.get(&actor).copied().unwrap_or(now);
        let charge = self
            .physical_drivers
            .get(&actor)
            .and_then(|d| d.started)
            .map_or(now, |t| t + 1.0);
        (deadline.max(charge) - now).max(0.0)
    }
    pub(crate) fn restore_physical_recovery(
        &mut self,
        actor: EntityId,
        now: f64,
        remaining: f64,
    ) -> Result<(), CombatRejection> {
        if !now.is_finite()
            || now < 0.0
            || !remaining.is_finite()
            || !(0.0..=180.0).contains(&remaining)
            || !(now + remaining).is_finite()
        {
            return Err(CombatRejection::InvalidRequest);
        }
        if !self.physical_deadlines.contains_key(&actor)
            && self.physical_deadlines.len() >= self.capacity
        {
            return Err(CombatRejection::Capacity);
        }
        let previous = self.physical_deadlines.get(&actor).copied().unwrap_or(now);
        self.physical_deadlines
            .insert(actor, previous.max(now + remaining));
        Ok(())
    }
    pub(crate) fn set_physical_options(
        &mut self,
        actor: EntityId,
        options1: u32,
        options2: u32,
    ) -> Result<(), CombatRejection> {
        if !self.physical.contains_key(&actor) {
            return Err(CombatRejection::MissingCombatProfile);
        }
        self.physical_options.insert(actor, (options1, options2));
        Ok(())
    }
    pub(super) fn enable_physical_driver(&mut self, actor: EntityId) {
        self.physical_drivers.entry(actor).or_insert(AttackDriver {
            current: None,
            mode_style: None,
            queued: None,
            due: 0.0,
            started: None,
            operation: 0,
            timeout: 0.0,
            controller: None,
            reload: None,
            transition: None,
            needs_reload: false,
            reload_speed: 1.0,
        });
    }
    pub(super) fn queue_automatic_mode_style(&mut self, actor: EntityId, style: Option<u32>) {
        if let Some(driver) = self.physical_drivers.get_mut(&actor) {
            driver.mode_style = style;
            driver.queued = None;
        }
    }
    pub(super) fn prepare_mode_style(
        &self,
        world: &World,
        actor: EntityId,
        mode: u32,
    ) -> Result<Option<u32>, CombatRejection> {
        if !self.physical_drivers.contains_key(&actor) {
            return Ok(None);
        }
        let Some(before) = world.source_motion_state(actor) else {
            return if world
                .body(actor)
                .is_ok_and(|b| b.collision_shape().is_none())
            {
                Ok(None)
            } else {
                Err(CombatRejection::MissingCombatProfile)
            };
        };
        let style = match mode {
            1 => 0x8000003d,
            8 => 0x80000049,
            _ => {
                self.physical
                    .get(&actor)
                    .ok_or(CombatRejection::MissingCombatProfile)?
                    .style
            }
        };
        if before.style == style {
            return Ok(None);
        }
        self.physical_chain(world, actor, style, 1.0)
            .ok_or(CombatRejection::MissingCombatProfile)?;
        Ok(Some(style))
    }
    pub(super) fn request_mode_style(
        &mut self,
        world: &mut World,
        actor: EntityId,
        style: Option<u32>,
    ) -> Result<(), CombatRejection> {
        if let Some(style) = style {
            if let Some(token) = world
                .source_motion_token(actor)
                .filter(|t| t.domain == bace_motion::MotionDomain::Physical)
            {
                super::faults::end_if_owned(world, actor, token.owner)
                    .map_err(|_| CombatRejection::Busy)?;
            }
            self.physical_drivers
                .get_mut(&actor)
                .ok_or(CombatRejection::MissingCombatProfile)?
                .mode_style = Some(style);
            match self.start_mode_style(world, actor, style) {
                Err(CombatRejection::Busy) => Ok(()),
                other => other,
            }
        } else {
            Ok(())
        }
    }
    fn start_mode_style(
        &mut self,
        world: &mut World,
        actor: EntityId,
        style: u32,
    ) -> Result<(), CombatRejection> {
        if world
            .source_motion_state(actor)
            .is_some_and(|s| s.style == style)
        {
            self.physical_drivers
                .get_mut(&actor)
                .expect("owner")
                .mode_style = None;
            return Ok(());
        }
        if self.physical_events.len() == self.capacity.max(2) {
            return Err(CombatRejection::Busy);
        }
        let operation = self
            .next_attack
            .checked_add(1)
            .ok_or(CombatRejection::Capacity)?;
        let motion = self
            .begin_physical_motion(world, actor, operation, style, 1.0)?
            .ok_or(CombatRejection::MissingCombatProfile)?;
        self.next_attack = operation;
        let driver = self.physical_drivers.get_mut(&actor).expect("owner");
        driver.transition = Some(motion);
        driver.mode_style = None;
        self.physical_events.push_back(PhysicalCombatEvent::Motion {
            actor,
            target: actor,
            motion: style,
            speed: 1.0,
        });
        Ok(())
    }
    pub(super) fn queue_physical_request(
        &mut self,
        world: &World,
        actor: EntityId,
        request: CombatRequest,
        now: f64,
    ) -> Result<CombatChange, CombatRejection> {
        let (target, height, power, mode) =
            request_values(request).ok_or(CombatRejection::InvalidRequest)?;
        if !now.is_finite()
            || now < 0.0
            || actor == target
            || !(1..=3).contains(&height)
            || !power.is_finite()
            || !(0.0..=1.0).contains(&power)
        {
            return Err(CombatRejection::InvalidRequest);
        }
        if world.combatant(actor).is_none_or(|s| s.mode() != mode) {
            return Err(CombatRejection::UnsupportedMode);
        }
        if world.combatant(target).is_none_or(|s| s.health() == 0) {
            return Err(CombatRejection::Dead);
        }
        let profile = self
            .physical
            .get(&actor)
            .ok_or(CombatRejection::MissingCombatProfile)?;
        let defender = self
            .physical
            .get(&target)
            .ok_or(CombatRejection::MissingCombatProfile)?;
        if !super::physical::runtime_permission(world, actor, target, profile, defender) {
            return Err(CombatRejection::InvalidRequest);
        }
        let duration = charge_seconds(power, mode == 2 && profile.style == 0x80000046)
            .map_err(|_| CombatRejection::InvalidRequest)?;
        let d = self
            .physical_drivers
            .get_mut(&actor)
            .ok_or(CombatRejection::MissingCombatProfile)?;
        // A fresh request may have built while the previous animation ran. Keep
        // that origin through CancelAttack and combat-mode cycles.
        if d.current.is_none() {
            d.due = d.started.map_or(now, |start| (start + duration).max(now));
            d.due = d
                .due
                .max(self.physical_deadlines.get(&actor).copied().unwrap_or(now));
            d.timeout = now + 15.0;
        }
        d.queued = Some(request);
        Ok(CombatChange::AttackStarted { target })
    }
    pub(super) fn step_physical_drivers(
        &mut self,
        world: &mut World,
        now: f64,
        inventory: Option<&crate::inventory::Inventory>,
    ) {
        self.physical_approach_scratch.clear();
        self.physical_approach_scratch
            .extend(self.physical_drivers.iter().filter_map(|(&actor, d)| {
                matches!(d.current, Some(CombatRequest::TargetedMelee { .. })).then_some(actor)
            }));
        for index in 0..self.physical_approach_scratch.len() {
            let actor = self.physical_approach_scratch[index];
            let Some(CombatRequest::TargetedMelee { target, .. }) =
                self.physical_drivers.get(&actor).and_then(|d| d.current)
            else {
                continue;
            };
            match self.approach_physical(world, actor, target, 2) {
                Ok(true) => self.stop_physical_approach(world, actor),
                Ok(false) => {}
                Err(_) => {
                    self.stop_physical_approach(world, actor);
                    if let Some(m) = self
                        .physical_attacks
                        .get(&actor)
                        .and_then(|a| a.motion.as_ref())
                        && super::faults::end_if_owned(world, actor, m.token.owner).is_err()
                    {
                        continue;
                    }
                    self.cancel(actor);
                }
            }
        }
        // Each scan is bounded by admitted actors. At most one new animation is
        // installed each turn; no per-tick allocation or unbounded request FIFO.
        for (&actor, d) in &mut self.physical_drivers {
            if self.physical_events.len().saturating_add(2) > self.capacity.max(2) {
                break;
            }
            if let Some(transition) = &d.transition
                && transition.complete.is_some()
            {
                if super::faults::end_if_owned(world, actor, transition.token.owner).is_err() {
                    continue;
                }
                if transition.complete == Some(false) {
                    d.queued = None;
                }
                d.transition = None;
            }
            if let Some(reload) = &d.reload
                && reload.complete.is_some()
            {
                if super::faults::end_if_owned(world, actor, reload.token.owner).is_err() {
                    continue;
                }
                let valid = reload.complete == Some(true)
                    && d.queued.is_some_and(|request| {
                        valid_next_target(world, &self.physical, actor, request)
                    });
                self.physical_events
                    .push_back(PhysicalCombatEvent::AttackDone {
                        actor,
                        actor_incarnation: incarnation(world, actor),
                    });
                if valid {
                    self.physical_events
                        .push_back(PhysicalCombatEvent::CommenceAttack {
                            actor,
                            actor_incarnation: incarnation(world, actor),
                        });
                } else {
                    d.queued = None;
                }
                d.reload = None;
            }
            if d.current.is_none()
                && d.queued.is_none()
                && let Some(control) = d.controller.take()
            {
                world.finish_server_move(actor, control);
                if let Ok(body) = world.body_mut(actor) {
                    body.finish_server_turn(control);
                }
            }
            let Some(current) = d.current else {
                continue;
            };
            let done = match current {
                CombatRequest::TargetedMelee { .. } => !self.physical_attacks.contains_key(&actor),
                CombatRequest::TargetedMissile { .. } => {
                    self.missiles.get(&d.operation).is_none_or(|m| {
                        m.launched
                            && now >= self.physical_deadlines.get(&actor).copied().unwrap_or(now)
                            && m.motion.as_ref().is_none_or(|m| m.complete == Some(true))
                    })
                }
                _ => true,
            };
            if !done {
                continue;
            }
            d.current = None;
            let Some((_, _, power, mode)) = request_values(current) else {
                continue;
            };
            let alive = valid_next_target(world, &self.physical, actor, current);
            if !alive {
                if mode == 4 {
                    self.physical_events
                        .push_back(PhysicalCombatEvent::AttackDone {
                            actor,
                            actor_incarnation: incarnation(world, actor),
                        });
                }
                d.queued = None;
                continue;
            }
            let repeat = self
                .physical_options
                .get(&actor)
                .is_some_and(|(o, _)| o & 2 != 0);
            let manual = d.queued.is_some();
            if !manual && repeat {
                d.queued = Some(current);
            }
            if d.queued
                .is_some_and(|next| !valid_next_target(world, &self.physical, actor, next))
            {
                d.queued = None;
            }
            if mode == 4 && d.queued.is_none() {
                self.physical_events
                    .push_back(PhysicalCombatEvent::AttackDone {
                        actor,
                        actor_incarnation: incarnation(world, actor),
                    });
            }
            if let Some(next) = d.queued {
                if mode == 2 {
                    self.physical_events
                        .push_back(PhysicalCombatEvent::CommenceAttack {
                            actor,
                            actor_incarnation: incarnation(world, actor),
                        });
                }
                let next_power = request_values(next).map_or(power, |v| v.2);
                let dual = mode == 2
                    && self
                        .physical
                        .get(&actor)
                        .is_some_and(|p| p.style == 0x80000046);
                let charge = charge_seconds(next_power, dual).unwrap_or(1.0);
                d.due = if manual {
                    d.started.map_or(now, |s| (s + charge).max(now))
                } else {
                    now + charge
                };
                // GDLE missile Setup reserves one second for the reload link.
                if mode == 4 {
                    d.due = if manual {
                        d.due.max(now + 1.0)
                    } else {
                        d.due + 1.0
                    };
                    d.needs_reload = true;
                }
                d.timeout = now + 15.0;
            }
        }
        let mode_actor = self.physical_drivers.iter().find_map(|(&actor, d)| {
            d.mode_style
                .filter(|_| d.transition.is_none())
                .map(|style| (actor, style))
        });
        if let Some((actor, style)) = mode_actor {
            match self.start_mode_style(world, actor, style) {
                Ok(()) | Err(CombatRejection::Busy) => {}
                Err(error) => {
                    if self.physical_events.len() < self.capacity.max(2) {
                        self.physical_events.push_back(PhysicalCombatEvent::Fault {
                            actor,
                            actor_incarnation: incarnation(world, actor),
                            target: actor,
                            operation: 0,
                            error,
                        });
                        self.physical_drivers
                            .get_mut(&actor)
                            .expect("mode owner")
                            .mode_style = None;
                    }
                }
            }
        }
        let reload_actor = self.physical_drivers.iter().find_map(|(&id, d)| {
            (d.needs_reload && d.reload.is_none()).then_some((id, d.operation, d.reload_speed))
        });
        if let Some((actor, operation, speed)) = reload_actor {
            let duration = self
                .physical_chain(world, actor, 0x40000016, speed)
                .map_or(0.0, |c| c.nominal_duration_seconds());
            match self.begin_physical_motion_sequence(
                world,
                actor,
                0x40000016,
                speed,
                bace_motion::MotionToken {
                    domain: bace_motion::MotionDomain::Physical,
                    owner: operation,
                    sequence: 3,
                },
            ) {
                Ok(Some(motion)) => {
                    let deadline = self.physical_deadlines.entry(actor).or_insert(now);
                    *deadline = deadline.max(now + duration);
                    let d = self.physical_drivers.get_mut(&actor).expect("owner");
                    d.reload = Some(motion);
                    d.needs_reload = false;
                }
                Ok(None) | Err(CombatRejection::MissingCombatProfile) => {
                    if self.physical_events.len() < self.capacity.max(2) {
                        self.physical_drivers
                            .get_mut(&actor)
                            .expect("owner")
                            .cancel();
                        self.physical_events.push_back(PhysicalCombatEvent::Fault {
                            actor,
                            actor_incarnation: incarnation(world, actor),
                            target: actor,
                            operation,
                            error: CombatRejection::MissingCombatProfile,
                        });
                    }
                }
                Err(_) => {}
            }
        }
        let candidate = self
            .physical_drivers
            .iter()
            .filter(|(_, d)| {
                d.current.is_none()
                    && d.reload.is_none()
                    && d.transition.is_none()
                    && !d.needs_reload
                    && d.queued.is_some()
                    && now >= d.due
            })
            .min_by_key(|(id, _)| {
                (
                    self.physical_driver_cursor.is_some_and(|last| **id <= last),
                    **id,
                )
            })
            .map(|(&id, d)| (id, d.queued.expect("queued request"), d.timeout));
        let Some((actor, request, timeout)) = candidate else {
            return;
        };
        self.physical_driver_cursor = Some(actor);
        if self.physical_events.len() == self.capacity.max(2) {
            return;
        }
        let (target, height, power, mode) = request_values(request).expect("physical request");
        if let Some(before) = world.source_motion_state(actor)
            && let Some(profile) = self.physical.get(&actor)
            && (before.style != profile.style || before.substate != 0x41000003)
        {
            let style = if before.style == profile.style {
                0x41000003
            } else {
                profile.style
            };
            let Some(operation) = self.next_attack.checked_add(1) else {
                return;
            };
            match self.begin_physical_motion(world, actor, operation, style, 1.0) {
                Ok(Some(transition)) => {
                    self.next_attack = operation;
                    self.physical_drivers
                        .get_mut(&actor)
                        .expect("owner")
                        .transition = Some(transition);
                    self.physical_events.push_back(PhysicalCombatEvent::Motion {
                        actor,
                        target: actor,
                        motion: style,
                        speed: 1.0,
                    });
                }
                Err(CombatRejection::Busy) => {}
                _ => {
                    self.physical_drivers
                        .get_mut(&actor)
                        .expect("owner")
                        .cancel();
                    self.physical_events.push_back(PhysicalCombatEvent::Fault {
                        actor,
                        actor_incarnation: incarnation(world, actor),
                        target,
                        operation,
                        error: CombatRejection::MissingCombatProfile,
                    });
                }
            }
            return;
        }
        if now <= timeout {
            match self.approach_physical(world, actor, target, mode) {
                Ok(false) => return,
                Ok(true) => {}
                Err(error) => {
                    self.stop_physical_approach(world, actor);
                    self.physical_drivers
                        .get_mut(&actor)
                        .expect("owner")
                        .cancel();
                    self.physical_events.push_back(PhysicalCombatEvent::Fault {
                        actor,
                        actor_incarnation: incarnation(world, actor),
                        target,
                        operation: 0,
                        error,
                    });
                    return;
                }
            }
        }
        let result = if now > timeout {
            Err(CombatRejection::OutOfRange)
        } else if inventory.is_some_and(|i| self.validate_physical_equipment(actor, i).is_err()) {
            Err(CombatRejection::MissingCombatProfile)
        } else if mode == 2 {
            self.start_physical(world, actor, target, height, power, now)
        } else {
            self.start_missile(world, actor, target, height, power, now)
        };
        match result {
            Ok(_) => {
                self.stop_physical_approach(world, actor);
                let d = self
                    .physical_drivers
                    .get_mut(&actor)
                    .expect("retained owner");
                d.queued = None;
                d.current = Some(request);
                d.started = Some(now);
                d.operation = self.next_attack;
                if let Some(m) = self.missiles.get(&self.next_attack) {
                    d.reload_speed = m.selected.speed;
                }
            }
            Err(CombatRejection::Busy) => {}
            Err(error) => {
                self.stop_physical_approach(world, actor);
                self.physical_drivers
                    .get_mut(&actor)
                    .expect("retained owner")
                    .cancel();
                self.physical_events.push_back(PhysicalCombatEvent::Fault {
                    actor,
                    actor_incarnation: incarnation(world, actor),
                    target,
                    operation: 0,
                    error,
                });
            }
        }
    }
    pub(super) fn stop_physical_approach(&mut self, world: &mut World, actor: EntityId) {
        if let Some(control) = self
            .physical_drivers
            .get_mut(&actor)
            .and_then(|d| d.controller.take())
        {
            world.finish_server_move(actor, control);
            if let Ok(body) = world.body_mut(actor) {
                body.finish_server_turn(control);
            }
        }
    }
    fn approach_physical(
        &mut self,
        world: &mut World,
        actor: EntityId,
        target: EntityId,
        mode: u32,
    ) -> Result<bool, CombatRejection> {
        let (cell, source) = world
            .actor_state(actor)
            .map_err(|_| CombatRejection::MissingActor)?;
        let (target_cell, dest) = world
            .actor_state(target)
            .map_err(|_| CombatRejection::MissingActor)?;
        if cell != target_cell {
            return Err(CombatRejection::OutOfRange);
        }
        let offset = dest.position() - source.position();
        let distance = offset.length_squared().sqrt();
        if distance > if mode == 4 { 80.0 } else { 15.0 } {
            return Err(CombatRejection::OutOfRange);
        }
        let range = if mode == 4 {
            80.0
        } else {
            self.physical
                .get(&actor)
                .ok_or(CombatRejection::MissingCombatProfile)?
                .range
        };
        let (near, clear) = world
            .attack_geometry(actor, target, range)
            .map_err(|_| CombatRejection::MissingActor)?;
        let heading = (-offset.x).atan2(offset.y);
        let delta = (heading - source.heading_radians() + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        if near && clear && delta.abs() <= 0.1 {
            return Ok(true);
        }
        if !clear && near {
            return Err(CombatRejection::Obstructed);
        }
        let d = self.physical_drivers.get_mut(&actor).expect("driver");
        let existing = d.controller;
        let control = if let Some(control) = existing {
            control
        } else {
            world
                .next_server_control(actor)
                .map_err(|_| CombatRejection::Capacity)?
        };
        let axis = (delta / 0.25).clamp(-1.0, 1.0);
        let turn =
            bace_motion::TurnIntent::new(axis).map_err(|_| CombatRejection::InvalidRequest)?;
        let direction = if !near && delta.abs() < 0.5 {
            offset * (1.0 / distance.max(0.001))
        } else {
            bace_geometry::Vec3::ZERO
        };
        let intent = bace_motion::MotionIntent::new(direction, false)
            .map_err(|_| CombatRejection::InvalidRequest)?;
        if existing.is_some() {
            world
                .continue_server_move_scaled(actor, source.epoch(), control, intent, 1.5)
                .map_err(|_| CombatRejection::InvalidRequest)?;
            world
                .body_mut(actor)
                .map_err(|_| CombatRejection::MissingActor)?
                .continue_server_turn(source.epoch(), control, turn)
                .map_err(|_| CombatRejection::InvalidRequest)?;
        } else {
            world
                .begin_server_move_scaled(actor, source.epoch(), control, intent, 1.5)
                .map_err(|_| CombatRejection::InvalidRequest)?;
            if world
                .body_mut(actor)
                .map_err(|_| CombatRejection::MissingActor)?
                .begin_server_turn(source.epoch(), control, turn)
                .is_err()
            {
                world.finish_server_move(actor, control);
                return Err(CombatRejection::InvalidRequest);
            }
            d.controller = Some(control);
        }
        if let Some(run_rate) = world.body(actor).ok().and_then(|b| b.locomotion_run_rate()) {
            use bace_gameplay_api::visibility::{AcceptedMoveParameters, ServerMovementGoal};
            world
                .bind_movement_goal(
                    actor,
                    control,
                    ServerMovementGoal::MoveToObject {
                        target,
                        parameters: AcceptedMoveParameters {
                            flags: 0x1f09eff0,
                            distance_to_object: (range - 0.5).max(0.0),
                            min_distance: 0.1,
                            fail_distance: 15.0,
                            speed: 1.5,
                            walk_run_threshold: 15.0,
                            desired_heading: 0.0,
                        },
                        run_rate,
                    },
                )
                .map_err(|_| CombatRejection::InvalidRequest)?;
        }
        Ok(false)
    }
}
fn request_values(request: CombatRequest) -> Option<(EntityId, u32, f32, u32)> {
    match request {
        CombatRequest::TargetedMelee {
            target,
            height,
            power,
        } => Some((target, height, power, 2)),
        CombatRequest::TargetedMissile {
            target,
            height,
            accuracy,
        } => Some((target, height, accuracy, 4)),
        _ => None,
    }
}

fn valid_next_target(
    world: &World,
    profiles: &std::collections::BTreeMap<
        EntityId,
        std::sync::Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
    >,
    actor: EntityId,
    request: CombatRequest,
) -> bool {
    let Some((target, _, _, mode)) = request_values(request) else {
        return false;
    };
    !world.is_in_portal_transit(actor)
        && !world.is_in_portal_transit(target)
        && world
            .combatant(actor)
            .is_some_and(|s| s.health() > 0 && s.mode() == mode)
        && world.combatant(target).is_some_and(|s| s.health() > 0)
        && profiles
            .get(&actor)
            .zip(profiles.get(&target))
            .is_some_and(|(a, b)| super::physical::runtime_permission(world, actor, target, a, b))
}
