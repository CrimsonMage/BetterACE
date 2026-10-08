//! Generator owner integration; host requests are immutable and exact-key fenced.
use super::*;
#[path = "generator_equipment.rs"]
mod equipment;
#[path = "generator_lifecycle.rs"]
mod lifecycle;
#[path = "generator_nested.rs"]
mod nested;
#[path = "generator_region.rs"]
mod region;
use crate::generators::*;
use bace_gameplay_api::generators::*;
use bace_spawning::{GeneratorLimits, GeneratorMachine};
use std::sync::Arc;
impl Kernel {
    /// Production births wait for complete cold appearance/emote/equipment
    /// preparation, including creatures without authored equipment. Synthetic
    /// numeric harnesses may retain the original already-prepared fast path.
    pub fn require_prepared_generator_births(&mut self) -> Result<(), GeneratorServiceError> {
        if self.generators.has_state() {
            return Err(GeneratorServiceError::Busy);
        }
        self.generators.prepared_births = true;
        Ok(())
    }
    pub fn configure_generators(
        &mut self,
        root: Arc<bace_random::RandomRoot>,
        unix_epoch: i64,
        is_day: bool,
    ) -> Result<(), GeneratorServiceError> {
        if self.generators.has_state() || unix_epoch < 0 {
            return Err(GeneratorServiceError::Busy);
        }
        self.population
            .configure_generator_loot(root.clone(), unix_epoch as u64)
            .map_err(|_| GeneratorServiceError::Invalid)?;
        self.generators.root = Some(root);
        self.generators.epoch = unix_epoch;
        self.generators.is_day = is_day;
        Ok(())
    }
    pub fn set_generator_day(&mut self, is_day: bool) {
        self.generators.is_day = is_day;
    }
    fn generator_clock(
        &mut self,
        event: Option<&str>,
    ) -> Result<GeneratorClock, GeneratorServiceError> {
        let now = self
            .generators
            .epoch
            .checked_add(i64::try_from(self.tick / 30).map_err(|_| GeneratorServiceError::Invalid)?)
            .ok_or(GeneratorServiceError::Invalid)?;
        let event = if let Some(name) = event {
            self.npcs
                .generator_event(
                    name,
                    i32::try_from(now).map_err(|_| GeneratorServiceError::Invalid)?,
                )
                .map_err(|_| GeneratorServiceError::Invalid)?
        } else {
            GeneratorEventState::Missing
        };
        Ok(GeneratorClock {
            tick: self.tick,
            unix_seconds: now,
            is_day: self.generators.is_day,
            event,
        })
    }
    pub fn register_generator(
        &mut self,
        definition: Arc<GeneratorDefinition>,
    ) -> Result<(), GeneratorServiceError> {
        if self.generators.root.is_none() {
            return Err(GeneratorServiceError::Missing);
        }
        let id = definition.identity.entity;
        if self.generators.machines.contains_key(&id) {
            return Err(GeneratorServiceError::Stale);
        }
        if self.generators.machines.len() == self.generators.capacity {
            return Err(GeneratorServiceError::Capacity);
        }
        let clock = self.generator_clock(definition.event.as_deref())?;
        let machine = GeneratorMachine::new(definition, clock, GeneratorLimits::default())?;
        self.generators.machines.insert(id, machine);
        Ok(())
    }
    pub fn register_generated_npc_template(
        &mut self,
        revision: u64,
        template: u32,
        value: GeneratedNpcTemplate,
    ) -> Result<(), GeneratorServiceError> {
        if revision == 0 || template == 0 || self.generators.templates.len() >= 4096 {
            return Err(GeneratorServiceError::Capacity);
        }
        if self
            .generators
            .templates
            .contains_key(&(revision, template))
        {
            return Err(GeneratorServiceError::Stale);
        }
        if let Some(profile) = &value.physical {
            bace_combat::physical::validate_physical_profile(profile)
                .map_err(|_| GeneratorServiceError::Invalid)?;
        }
        self.generators
            .templates
            .insert((revision, template), value);
        Ok(())
    }
    pub fn supply_generator_id(&mut self, id: EntityId) -> Result<(), GeneratorServiceError> {
        if !self.available_generator_id(id) {
            return Err(GeneratorServiceError::Invalid);
        }
        if self.generators.ids.len() == self.generators.capacity {
            return Err(GeneratorServiceError::Capacity);
        }
        self.generators.ids.push_back(id);
        Ok(())
    }
    pub fn generator_control(
        &mut self,
        identity: GeneratorIdentity,
        control: GeneratorControl,
    ) -> Result<(), GeneratorServiceError> {
        if self.generator_inventory_busy(identity) {
            return Err(GeneratorServiceError::Busy);
        }
        if self
            .generators
            .requests
            .keys()
            .any(|key| key.generator == identity)
        {
            return Err(GeneratorServiceError::Busy);
        }
        let mut machine = self
            .generators
            .machines
            .get(&identity.entity)
            .filter(|m| m.accepts_identity(identity))
            .cloned()
            .ok_or(GeneratorServiceError::Stale)?;
        let clock = self.generator_clock(machine.definition().event.as_deref())?;
        let transition = match control {
            GeneratorControl::Generate => machine.generate_now(
                clock,
                self.generators
                    .root
                    .as_ref()
                    .ok_or(GeneratorServiceError::Missing)?,
            )?,
            GeneratorControl::Activate => machine.activate(
                clock,
                self.generators
                    .root
                    .as_ref()
                    .ok_or(GeneratorServiceError::Missing)?,
            )?,
            GeneratorControl::Reset => machine.reset(clock)?,
            GeneratorControl::Regenerate => machine.regenerate(
                clock,
                self.generators
                    .root
                    .as_ref()
                    .ok_or(GeneratorServiceError::Missing)?,
            )?,
            GeneratorControl::Death => machine.death(clock)?,
            GeneratorControl::Destroy => machine.destroy(clock)?,
            GeneratorControl::Unload => machine.unload(clock)?,
        };
        self.generators.adopt(identity.entity, machine, transition)
    }
    pub fn notify_generated_entity(
        &mut self,
        entity: EntityId,
        notification: GeneratorNotification,
    ) -> Result<bool, GeneratorServiceError> {
        let owner = self
            .generators
            .machines
            .iter()
            .find(|(_, m)| m.owns_member(entity))
            .map(|(&id, _)| id);
        let Some(owner) = owner else {
            return Ok(false);
        };
        if self.generator_inventory_busy(self.generators.machines[&owner].definition().identity) {
            return Err(GeneratorServiceError::Busy);
        }
        let mut machine = self.generators.machines[&owner].clone();
        let clock = self.generator_clock(machine.definition().event.as_deref())?;
        let transition = machine.notify(entity, notification, clock)?;
        self.generators.adopt(owner, machine, transition)?;
        Ok(true)
    }
    pub fn take_generator_request(&mut self) -> Option<GeneratorHostRequest> {
        let key = *self
            .generators
            .requests
            .keys()
            .find(|key| !self.generators.submitted.contains(key))?;
        self.refresh_generator_request(key).ok()
    }
    pub fn refresh_generator_request(
        &mut self,
        key: GeneratorSpawnKey,
    ) -> Result<GeneratorHostRequest, GeneratorServiceError> {
        let request = self
            .generators
            .requests
            .get(&key)
            .ok_or(GeneratorServiceError::Stale)?;
        let slots = if let GeneratorDestination::Contain { container } = request.intent.destination
        {
            self.inventory.container(container).map(|_| {
                let mut main = 0;
                let mut pack = 0;
                for item in self.inventory.items().filter(|i| matches!(i.place,
                    bace_inventory::ItemPlace::Contained { container: owner, equipped: 0, .. } if owner == container)) {
                    if item.pack_slot { pack += 1; } else { main += 1; }
                }
                (main, pack)
            })
        } else {
            None
        };
        let request = self
            .generators
            .requests
            .get_mut(&key)
            .expect("selected request");
        request.next_slots = slots;
        let request = request.clone();
        self.generators.submitted.insert(key);
        Ok(request)
    }
    pub fn retry_generator_request(
        &mut self,
        key: GeneratorSpawnKey,
    ) -> Result<(), GeneratorServiceError> {
        if !self.generators.requests.contains_key(&key) {
            return Err(GeneratorServiceError::Stale);
        }
        self.generators.submitted.remove(&key);
        Ok(())
    }
    pub fn confirm_generator_spawn(
        &mut self,
        receipt: GeneratorSpawnReceipt,
    ) -> Result<(), GeneratorServiceError> {
        if !self.generators.submitted.contains(&receipt.key)
            || !self.generators.requests.contains_key(&receipt.key)
        {
            return Err(GeneratorServiceError::Stale);
        }
        let id = receipt.key.generator.entity;
        let mut machine = self
            .generators
            .machines
            .get(&id)
            .cloned()
            .ok_or(GeneratorServiceError::Stale)?;
        let blocked = matches!(receipt.result, GeneratorSpawnResult::Blocked(_));
        let key = receipt.key;
        let transition = machine.acknowledge(receipt)?;
        self.generators.adopt(id, machine, transition)?;
        self.generators.submitted.remove(&key);
        if !blocked {
            self.generators.requests.remove(&key);
        }
        Ok(())
    }
    pub fn take_generator_event(&mut self) -> Option<GeneratorWorldEvent> {
        self.generators.events.pop_front()
    }
    pub fn take_generator_lifecycle(&mut self) -> Option<GeneratorLifecycleEffect> {
        self.generators.effects.front().cloned()
    }
    pub fn has_generator_state(&self) -> bool {
        self.generators.has_state()
            || !self.generator_outcomes.is_empty()
            || !self.generated_vendors.is_empty()
            || self.generated_enchantments.has_state()
            || self.constructed_creatures.has_state()
            || !self.generated_retirements.is_empty()
    }
    pub fn generator_reserves_identity(&self, id: EntityId) -> bool {
        self.generators.reserves(id) || self.generated_vendor_reserves_identity(id)
    }
    pub(crate) fn step_generators(&mut self) -> Result<(), SimulationError> {
        self.step_generators_inner()
            .map_err(|_| SimulationError::InvalidCommand)
    }
    fn step_generators_inner(&mut self) -> Result<(), GeneratorServiceError> {
        if self.generators.root.is_none() {
            return Ok(());
        }
        self.process_generator_lifecycle()?;
        // Durable death acknowledgment is retained until generator admission succeeds.
        while let Some(death) = self.population.generated_death() {
            let valid = self
                .generators
                .machines
                .get(&death.origin.generator)
                .is_some_and(|m| {
                    m.accepts_revision(death.origin.incarnation, death.origin.content_revision)
                });
            match self.retire_generated_creature_equipment(death.actor) {
                Ok(()) => {}
                Err(GeneratorServiceError::Busy | GeneratorServiceError::Capacity) => break,
                Err(error) => return Err(error),
            }
            self.combat.retire_actor(death.actor);
            if valid {
                match self.notify_generated_entity(death.actor, GeneratorNotification::Destruction)
                {
                    Ok(_) => {}
                    Err(GeneratorServiceError::Capacity | GeneratorServiceError::Busy) => break,
                    Err(e) => return Err(e),
                }
            }
            self.population.acknowledge_generated_death(death);
        }
        self.generators.scratch.clear();
        self.generators
            .scratch
            .extend(self.generators.machines.keys().copied());
        for index in 0..self.generators.scratch.len() {
            let id = self.generators.scratch[index];
            if self
                .region_residency
                .state((self.generators.machines[&id].definition().location.cell >> 16) as u16)
                .is_some_and(|s| s.phase == crate::RegionPhase::Draining)
            {
                continue;
            }
            if self.generator_inventory_busy(self.generators.machines[&id].definition().identity) {
                continue;
            }
            if self.generators.effects.len() >= self.generators.capacity
                || self.generators.requests.len() >= self.generators.capacity
            {
                break;
            }
            if self.generators.machines[&id]
                .next_wake_tick()
                .is_some_and(|wake| wake <= self.tick)
            {
                let mut machine = self.generators.machines[&id].clone();
                let clock = self.generator_clock(machine.definition().event.as_deref())?;
                let transition =
                    machine.advance(clock, self.generators.root.as_ref().expect("configured"))?;
                match self.generators.adopt(id, machine, transition) {
                    Ok(()) => {}
                    Err(GeneratorServiceError::Capacity) => continue,
                    Err(error) => return Err(error),
                }
            }
            let cursor = self.generators.cursors.get(&id).copied();
            let machine = &self.generators.machines[&id];
            let eligible =
                |intent: &GeneratorSpawnIntent| !self.generators.requests.contains_key(&intent.key);
            let Some(intent) = machine
                .next_spawn_matching(|intent| {
                    eligible(intent) && cursor.is_none_or(|last| intent.key > last)
                })
                .or_else(|| machine.next_spawn_matching(eligible))
                .cloned()
            else {
                continue;
            };
            self.generators.cursors.insert(id, intent.key);
            if self.generators.requests.contains_key(&intent.key) {
                continue;
            }
            let Some(&entity) = self.generators.ids.front() else {
                continue;
            };
            if let Some(template) = self
                .generators
                .templates
                .get(&(
                    intent.key.generator.content_revision,
                    intent.profile.weenie_class_id,
                ))
                .cloned()
                && !self.generators.prepared_births
                && !template.requires_equipment
                && intent.profile.where_create & 0x40 == 0
                && matches!(
                    intent.destination,
                    GeneratorDestination::Default(_)
                        | GeneratorDestination::Specific(_)
                        | GeneratorDestination::Scatter { .. }
                )
            {
                if self.generators.events.len() == self.generators.capacity {
                    continue;
                }
                let mut admitted = self.generators.machines[&id].clone();
                let accepted = admitted.acknowledge(GeneratorSpawnReceipt {
                    key: intent.key,
                    result: GeneratorSpawnResult::Completed {
                        members: vec![GeneratorSpawnMember {
                            entity,
                            contribution: 1,
                        }],
                        materialized: true,
                        failed_placements: 0,
                    },
                })?;
                if accepted.effects.len() > self.generators.capacity - self.generators.effects.len()
                {
                    continue;
                }
                match self.spawn_generator_npc(&intent, entity, template) {
                    Ok(location) => {
                        let birth = match self.prepare_generator_birth(entity) {
                            Ok(birth) => birth,
                            Err(error) => {
                                self.rollback_staged_generator_npc(entity)?;
                                return Err(error);
                            }
                        };
                        self.generators.ids.pop_front();
                        self.generators.adopt(id, admitted, accepted)?;
                        self.confirm_npc_combat_assets(entity);
                        self.generators
                            .events
                            .push_back(GeneratorWorldEvent::Spawned {
                                birth,
                                key: intent.key,
                                entity,
                                template: intent.profile.weenie_class_id,
                                location,
                            });
                    }
                    Err(
                        GeneratorServiceError::Geometry
                        | GeneratorServiceError::Capacity
                        | GeneratorServiceError::Missing,
                    ) => {}
                    Err(GeneratorServiceError::Placement) => {
                        let mut failed = self.generators.machines[&id].clone();
                        let transition = failed.acknowledge(GeneratorSpawnReceipt {
                            key: intent.key,
                            result: GeneratorSpawnResult::Completed {
                                members: vec![],
                                materialized: true,
                                failed_placements: 1,
                            },
                        })?;
                        self.generators.adopt(id, failed, transition)?;
                        self.generators.ids.pop_front();
                    }
                    Err(error) => return Err(error),
                }
            } else {
                self.generators.ids.pop_front();
                let landblock =
                    (self.generators.machines[&id].definition().location.cell >> 16) as u16;
                self.generators.requests.insert(
                    intent.key,
                    GeneratorHostRequest {
                        landblock,
                        next_slots: None,
                        intent,
                        entities: vec![entity],
                    },
                );
            }
        }
        Ok(())
    }
}
impl Kernel {
    pub(super) fn spawn_generator_npc(
        &mut self,
        intent: &GeneratorSpawnIntent,
        entity: EntityId,
        template: GeneratedNpcTemplate,
    ) -> Result<GeneratorLocation, GeneratorServiceError> {
        let location = self.stage_generator_npc(intent, entity, template, 0)?;
        match self.prepare_nested_generators(
            intent,
            &[(entity, intent.profile.weenie_class_id, location)],
        ) {
            Ok(nested) => {
                self.adopt_nested_generators(nested);
                Ok(location)
            }
            Err(error) => {
                self.rollback_staged_generator_npc(entity)?;
                Err(error)
            }
        }
    }
    /// Atomic geometry/definition generation adoption. Cold work is complete;
    /// current actors must validate before any generator generation is replaced.
    pub fn install_generator_region(
        &mut self,
        landblock: u16,
        epoch: u64,
        revision: u64,
        geometry: Arc<bace_physics::GeometryRegion>,
        definitions: Vec<Arc<GeneratorDefinition>>,
    ) -> Result<(), GeneratorServiceError> {
        if epoch == 0
            || revision == 0
            || self.generators.root.is_none()
            || self
                .generators
                .activation_fences
                .get(&landblock)
                .is_some_and(|(old, _)| *old >= epoch)
        {
            return Err(GeneratorServiceError::Stale);
        }
        if definitions.len() > self.generators.capacity - self.generators.machines.len() {
            return Err(GeneratorServiceError::Capacity);
        }
        let mut staged = std::collections::BTreeMap::new();
        for definition in definitions {
            if definition.location.cell >> 16 != u32::from(landblock)
                || definition.identity.incarnation != epoch
                || definition.identity.content_revision != revision
                || self
                    .generators
                    .machines
                    .contains_key(&definition.identity.entity)
                || staged.contains_key(&definition.identity.entity)
            {
                return Err(GeneratorServiceError::Stale);
            }
            let clock = self.generator_clock(definition.event.as_deref())?;
            staged.insert(
                definition.identity.entity,
                GeneratorMachine::new(definition, clock, GeneratorLimits::default())?,
            );
        }
        let geometry = if let Some(current) = self.world.geometry() {
            Arc::new(
                current
                    .replace_landblock(landblock, &geometry)
                    .map_err(|_| GeneratorServiceError::Geometry)?,
            )
        } else {
            geometry
        };
        self.world
            .install_geometry(geometry)
            .map_err(|_| GeneratorServiceError::Geometry)?;
        self.generators.machines.extend(staged);
        self.generators
            .activation_fences
            .insert(landblock, (epoch, revision));
        Ok(())
    }
}
impl Kernel {
    /// Extend a materialized treasure/container ticket with already allocated,
    /// owner-reserved IDs. Retrying the same required total never consumes more.
    pub fn reserve_generator_request_ids(
        &mut self,
        key: GeneratorSpawnKey,
        total: usize,
    ) -> Result<GeneratorHostRequest, GeneratorServiceError> {
        let old = self
            .generators
            .requests
            .get(&key)
            .ok_or(GeneratorServiceError::Stale)?
            .entities
            .len();
        if total < old || total > 1024 {
            return Err(GeneratorServiceError::Invalid);
        }
        let needed = total - old;
        if self.generators.ids.len() < needed {
            return Err(GeneratorServiceError::Capacity);
        }
        let request = self
            .generators
            .requests
            .get_mut(&key)
            .expect("checked ticket");
        request.entities.extend(self.generators.ids.drain(..needed));
        Ok(request.clone())
    }
    /// Pure preparation for the item owner composite; insertion/receipt adoption
    /// must occur together after all inventory/container participants preflight.
    pub fn prepare_generated_world_actor(
        &self,
        key: GeneratorSpawnKey,
        entity: EntityId,
        shape: Arc<bace_physics::CollisionShape>,
    ) -> Result<bace_entity::Actor, GeneratorServiceError> {
        let request = self
            .generators
            .requests
            .get(&key)
            .filter(|r| r.entities.contains(&entity))
            .ok_or(GeneratorServiceError::Stale)?;
        if !self.generators.submitted.contains(&key) {
            return Err(GeneratorServiceError::Stale);
        }
        let ordinal = request
            .entities
            .iter()
            .position(|id| *id == entity)
            .ok_or(GeneratorServiceError::Stale)?;
        self.prepare_generated_world_actor_ordinal(key, entity, shape, ordinal as u32)
    }
    pub(super) fn prepare_generated_world_actor_ordinal(
        &self,
        key: GeneratorSpawnKey,
        entity: EntityId,
        shape: Arc<bace_physics::CollisionShape>,
        ordinal: u32,
    ) -> Result<bace_entity::Actor, GeneratorServiceError> {
        let request = self
            .generators
            .requests
            .get(&key)
            .filter(|r| r.entities.contains(&entity) && self.generators.submitted.contains(&key))
            .ok_or(GeneratorServiceError::Stale)?;
        let locations = self.generator_placement_candidates(&request.intent, ordinal)?;
        for location in locations {
            let q = location.rotation;
            let norm = q.iter().map(|v| v * v).sum::<f32>();
            if !norm.is_finite()
                || !(0.999..=1.001).contains(&norm)
                || q[0].abs() > 0.0002
                || q[1].abs() > 0.0002
            {
                return Err(GeneratorServiceError::Geometry);
            }
            if let Ok(body) = self
                .world
                .prepare_geometry_body(bace_physics::GeometrySpawn {
                    cell: location.cell,
                    position: bace_geometry::Vec3::new(
                        location.origin[0],
                        location.origin[1],
                        location.origin[2],
                    ),
                    shape: shape.clone(),
                    capabilities: bace_motion::Capabilities {
                        speed: 0.0,
                        jump_impulse: 0.0,
                    },
                    heading: 2.0 * q[2].atan2(q[3]),
                    maximum_turn_rate: 0.0,
                })
            {
                let actor = bace_entity::Actor {
                    id: entity,
                    cell: bace_types::CellId(location.cell),
                    body,
                };
                if self.world.validate_actor(&actor).is_ok() {
                    return Ok(actor);
                }
            }
        }
        Err(GeneratorServiceError::Placement)
    }
    pub(super) fn generator_placement_candidates(
        &self,
        intent: &GeneratorSpawnIntent,
        ordinal: u32,
    ) -> Result<Vec<GeneratorLocation>, GeneratorServiceError> {
        let region = self
            .world
            .geometry()
            .ok_or(GeneratorServiceError::Geometry)?;
        let (center, radius, attempts) = match intent.destination {
            GeneratorDestination::Default(p) | GeneratorDestination::Specific(p) => {
                if region.cell(p.cell).is_none() {
                    return Err(GeneratorServiceError::Geometry);
                }
                return Ok(vec![p]);
            }
            GeneratorDestination::Scatter {
                center,
                radius,
                attempts,
            } => (center, radius, attempts.min(20)),
            _ => return Err(GeneratorServiceError::Invalid),
        };
        let mut stream = bace_spawning::generator_event_stream(
            self.generators
                .root
                .as_ref()
                .ok_or(GeneratorServiceError::Missing)?,
            intent,
            ordinal
                .checked_add(1)
                .ok_or(GeneratorServiceError::Invalid)?,
        )?;
        let mut candidates = Vec::with_capacity(attempts as usize);
        for _ in 0..attempts {
            let mut candidate = center;
            for i in 0..2 {
                candidate.origin[i] += (((stream
                    .next_u64()
                    .map_err(|_| GeneratorServiceError::Invalid)?
                    >> 40) as f32
                    / 16_777_216.0)
                    * 2.0
                    - 1.0)
                    * radius;
            }
            if candidate.cell & 0xffff < 0x100 {
                candidate.origin[0] = candidate.origin[0].clamp(0.5, 191.5);
                candidate.origin[1] = candidate.origin[1].clamp(0.5, 191.5);
                candidate.cell = (candidate.cell & 0xffff0000)
                    | (1 + (candidate.origin[0] / 24.0).floor() as u32 * 8
                        + (candidate.origin[1] / 24.0).floor() as u32);
                let p = bace_geometry::Vec3::new(
                    candidate.origin[0],
                    candidate.origin[1],
                    candidate.origin[2],
                );
                let (z, n) = region
                    .ground_at(candidate.cell, p)
                    .map_err(|_| GeneratorServiceError::Geometry)?;
                if n.z < 0.66417414 {
                    continue;
                }
                if !region
                    .has_building(candidate.cell)
                    .map_err(|_| GeneratorServiceError::Geometry)?
                    && (candidate.origin[2] - (z + 0.05)).abs() <= 10.0
                {
                    candidate.origin[2] = z + 0.05;
                }
            } else {
                let Some(cell) = region.indoor_cell_at(
                    candidate.cell >> 16,
                    bace_geometry::Vec3::new(
                        candidate.origin[0],
                        candidate.origin[1],
                        candidate.origin[2],
                    ),
                ) else {
                    continue;
                };
                candidate.cell = cell;
            }
            candidates.push(candidate);
        }
        Ok(candidates)
    }
}
