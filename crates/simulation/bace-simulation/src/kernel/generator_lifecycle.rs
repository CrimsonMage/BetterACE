//! Lifecycle effects remain in the bounded owner FIFO until their mutation succeeds.
use super::*;
impl Kernel {
    pub(in crate::kernel) fn process_generator_lifecycle(
        &mut self,
    ) -> Result<(), GeneratorServiceError> {
        let initial = self.generators.effects.len();
        let mut budget = self.generators.capacity;
        for _ in 0..initial {
            let Some(effect) = self.generators.effects.front().cloned() else {
                break;
            };
            if self.generators.events.len() == self.generators.capacity {
                break;
            }
            match self.apply_generator_lifecycle(&effect, &mut budget, 0) {
                Ok(()) => {
                    budget -= 1;
                    self.generators.effects.pop_front();
                    self.generators
                        .events
                        .push_back(GeneratorWorldEvent::Lifecycle(effect));
                }
                Err(GeneratorServiceError::Busy | GeneratorServiceError::Capacity) => break,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    /// Child transitions have their own bounded frame instead of expanding the
    /// full outer FIFO. Cleanup makes depth-first progress even when that FIFO
    /// is full; a blocked child retains its exact effect and ancestor frames.
    fn drain_nested_generator(
        &mut self,
        entity: EntityId,
        from_unload: bool,
        budget: &mut usize,
        depth: usize,
    ) -> Result<(), GeneratorServiceError> {
        if depth > 64 {
            return Err(GeneratorServiceError::Capacity);
        }
        if !self.generators.lifecycle_frames.contains_key(&entity) {
            let Some(mut child) = self.generators.machines.get(&entity).cloned() else {
                return Ok(());
            };
            if self.generators.lifecycle_frames.len() == self.generators.capacity {
                return Err(GeneratorServiceError::Capacity);
            }
            let clock = self.generator_clock(child.definition().event.as_deref())?;
            let transition = if from_unload {
                child.unload(clock)?
            } else {
                child.destroy(clock)?
            };
            self.generators
                .lifecycle_frames
                .insert(entity, transition.effects.into());
            self.generators.machines.insert(entity, child);
        }
        loop {
            let effect = self
                .generators
                .lifecycle_frames
                .get(&entity)
                .and_then(|frame| frame.front())
                .cloned();
            let Some(effect) = effect else {
                self.generators.lifecycle_frames.remove(&entity);
                return Ok(());
            };
            if self.generators.events.len() == self.generators.capacity {
                return Err(GeneratorServiceError::Capacity);
            }
            self.apply_generator_lifecycle(&effect, budget, depth)?;
            *budget -= 1;
            self.generators
                .lifecycle_frames
                .get_mut(&entity)
                .expect("retained lifecycle frame")
                .pop_front();
            self.generators
                .events
                .push_back(GeneratorWorldEvent::Lifecycle(effect));
        }
    }
    fn apply_generator_lifecycle(
        &mut self,
        effect: &GeneratorLifecycleEffect,
        budget: &mut usize,
        depth: usize,
    ) -> Result<(), GeneratorServiceError> {
        if *budget == 0 || depth > 64 {
            return Err(GeneratorServiceError::Capacity);
        }
        match effect {
            GeneratorLifecycleEffect::DestroyMember {
                generator,
                member,
                recursive,
                from_unload,
                ..
            } => {
                if *recursive {
                    self.drain_nested_generator(member.entity, *from_unload, budget, depth + 1)?;
                }
                if *budget == 0 || self.generators.events.len() == self.generators.capacity {
                    return Err(GeneratorServiceError::Capacity);
                }
                if self.world.contains_identity(member.entity)
                    && !self.npcs.can_retire_idle_source(member.entity)
                {
                    return Err(GeneratorServiceError::Busy);
                }
                let item_applied = self.apply_generated_item_lifecycle(effect)?;
                if !item_applied {
                    if let Some(origin) = self.population.generated_origin(member.entity) {
                        if origin.generator != generator.entity
                            || origin.incarnation != generator.incarnation
                            || origin.content_revision != generator.content_revision
                        {
                            return Err(GeneratorServiceError::Stale);
                        }
                        self.population
                            .can_remove_generated(member.entity, origin)
                            .map_err(|_| GeneratorServiceError::Busy)?;
                        self.retire_generated_creature_equipment(member.entity)?;
                        self.population
                            .remove_generated(member.entity, origin, &mut self.world)
                            .map_err(|_| GeneratorServiceError::Busy)?;
                        self.combat.retire_actor(member.entity);
                        self.npcs
                            .retire_idle_source(member.entity)
                            .expect("preflighted idle script");
                    } else if self.world.contains_identity(member.entity) {
                        // An unrelated or durable actor must never be mistaken for an old child.
                        return Err(GeneratorServiceError::Busy);
                    }
                }
                Ok(())
            }
            GeneratorLifecycleEffect::KillMember { generator, member } => {
                let Some(origin) = self.population.generated_origin(member.entity) else {
                    return Ok(());
                };
                if origin.generator != generator.entity
                    || origin.incarnation != generator.incarnation
                    || origin.content_revision != generator.content_revision
                {
                    return Err(GeneratorServiceError::Stale);
                }
                let state = self
                    .world
                    .combatant(member.entity)
                    .ok_or(GeneratorServiceError::Missing)?;
                let amount = state.health();
                let maximum = state.profile().maximum_health;
                let target_incarnation = state.incarnation();
                let revision = if amount > 0 {
                    self.world
                        .apply_vital_batch(
                            &[bace_entity::VitalMutation {
                                actor: member.entity,
                                vital: bace_entity::EntityVital::Health,
                                before: amount,
                                after: 0,
                            }],
                            None,
                        )
                        .map_err(|_| GeneratorServiceError::Busy)?[0]
                        .revision
                } else {
                    state.revision()
                };
                let event = CombatEvent::Damage {
                    death_blow: None,
                    attacker: Some(generator.entity),
                    target: member.entity,
                    target_incarnation,
                    amount,
                    current: 0,
                    maximum,
                    killed: true,
                    revision,
                };
                self.population
                    .refresh_owned_loot(member.entity, &self.inventory)
                    .map_err(|_| GeneratorServiceError::Busy)?;
                let social = &self.social.directory;
                if !self.population.ingest(
                    event,
                    &self.world,
                    &mut self.characters,
                    self.tick,
                    (self.allegiances.level_table.is_some(), true),
                    |id| social.presence(id).is_some_and(|p| p.olthoi),
                ) {
                    return Err(GeneratorServiceError::Busy);
                }
                // The dead body/registry still owns its trusted death program
                // and durable corpse continuation. Final destruction retires assets.
                self.combat.cancel(member.entity);
                Ok(())
            }
            GeneratorLifecycleEffect::DestroySelf(identity) => {
                let retire_script = self.world.contains_identity(identity.entity);
                if retire_script
                    && (!self.npcs.can_retire_idle_source(identity.entity)
                        || self.world.health_observation_pending_for(identity.entity)
                        || self.world.has_reserved_vitals(identity.entity)
                        || self.world.retirement_hold(identity.entity).is_some()
                        || !self.world.can_retire_actor_motion(identity.entity))
                {
                    return Err(GeneratorServiceError::Busy);
                }
                if self
                    .generators
                    .machines
                    .get(&identity.entity)
                    .is_some_and(|m| m.definition().identity != *identity)
                {
                    return Err(GeneratorServiceError::Stale);
                }
                if let Some(origin) = self.population.generated_origin(identity.entity) {
                    self.population
                        .can_remove_generated(identity.entity, origin)
                        .map_err(|_| GeneratorServiceError::Busy)?;
                    self.retire_generated_creature_equipment(identity.entity)?;
                    self.population
                        .remove_generated(identity.entity, origin, &mut self.world)
                        .map_err(|_| GeneratorServiceError::Busy)?;
                    self.combat.retire_actor(identity.entity);
                }
                if self
                    .generated_vendors
                    .get(&identity.entity)
                    .is_some_and(|v| !v.stock.items().is_empty())
                {
                    return Err(GeneratorServiceError::Busy);
                }
                self.inventory
                    .retire_generated_container(identity.entity)
                    .map_err(|_| GeneratorServiceError::Busy)?;
                self.world.remove(identity.entity);
                if retire_script {
                    self.npcs
                        .retire_idle_source(identity.entity)
                        .expect("preflighted idle script");
                }
                self.generated_vendors.remove(&identity.entity);
                self.generators.machines.remove(&identity.entity);
                self.generators.cursors.remove(&identity.entity);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
#[cfg(test)]
#[path = "generator_lifecycle/tests.rs"]
mod tests;
