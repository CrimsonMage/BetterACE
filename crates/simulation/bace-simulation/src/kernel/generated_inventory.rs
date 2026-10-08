//! Transient item admission and atomic first-acquisition owner completion.
use super::*;
use crate::GeneratorServiceError as G;
use bace_gameplay_api::InventoryRejection as E;
use bace_gameplay_api::{
    GeneratorClock, GeneratorEventState, GeneratorNotification, GeneratorSpawnKey,
    GeneratorSpawnMember, GeneratorSpawnReceipt, GeneratorSpawnResult,
};
impl Kernel {
    pub fn generated_inventory_items(&self, operation: u64) -> Result<Vec<EntityId>, E> {
        self.inventory.generated_changes(operation)
    }
    pub fn generator_inventory_busy(&self, identity: bace_gameplay_api::GeneratorIdentity) -> bool {
        if self
            .generators
            .lifecycle_frames
            .contains_key(&identity.entity)
        {
            return true;
        }
        self.generators.effects.iter().any(|effect| matches!(effect,bace_gameplay_api::GeneratorLifecycleEffect::DestroyMember{generator,..}|bace_gameplay_api::GeneratorLifecycleEffect::KillMember{generator,..} if *generator==identity)) || self.generators.machines.get(&identity.entity).is_some_and(|machine|machine.accepts_identity(identity)
            && self.inventory.items().any(|item|{
                if !self.inventory.reserved(item.id){return false;}
                if machine.owns_member(item.id){return true;}
                let mut place=item.place;
                for _ in 0..64 {
                    let bace_inventory::ItemPlace::Contained{container,..}=place else{break;};
                    if machine.owns_member(container){return true;}
                    let Some(parent)=self.inventory.item(container)else{break;};
                    place=parent.place;
                }
                false
            }))
    }
    pub(super) fn inventory_generator_destruction_pending(&self, operation: u64) -> bool {
        self.inventory
            .pending_ticket(operation)
            .is_some_and(|ticket| {
                self.generators
                    .effects
                    .iter()
                    .chain(self.generators.lifecycle_frames.values().flatten())
                    .any(|effect| {
                        let entity = match effect {
                            bace_gameplay_api::GeneratorLifecycleEffect::DestroyMember {
                                member,
                                ..
                            } => member.entity,
                            _ => return false,
                        };
                        ticket
                            .proposal
                            .participants
                            .iter()
                            .any(|(id, _)| *id == entity)
                    })
            })
    }
    /// Caller has already performed authoritative physical placement, or exact
    /// contain placement. Spawn registration and its generator receipt adopt
    /// together; capacity failure retains the immutable host request and IDs.
    pub fn admit_generated_inventory(
        &mut self,
        key: GeneratorSpawnKey,
        items: &[bace_inventory::InventoryItem],
        containers: &[bace_inventory::InventoryContainer],
    ) -> Result<(), G> {
        self.admit_generated_inventory_with_births(key, items, containers, None)
    }
    fn admit_generated_inventory_with_births(
        &mut self,
        key: GeneratorSpawnKey,
        items: &[bace_inventory::InventoryItem],
        containers: &[bace_inventory::InventoryContainer],
        births: Option<&super::generator_births::Births>,
    ) -> Result<(), G> {
        let request = self.generators.requests.get(&key).ok_or(G::Stale)?;
        if !self.generators.submitted.contains(&key)
            || items.len() != request.entities.len()
            || items.iter().any(|item| {
                !request.entities.contains(&item.id) || self.inventory.item(item.id).is_some()
            })
        {
            return Err(G::Invalid);
        }
        match request.intent.destination {
            bace_gameplay_api::GeneratorDestination::Contain{container}=>{
                if items.iter().any(|item|!matches!(item.place,bace_inventory::ItemPlace::Contained{container:c,equipped:0,..} if c==container)){return Err(G::Invalid);}
            }
            bace_gameplay_api::GeneratorDestination::Shop{..}=>return Err(G::Invalid),
            _=>{
                if items.iter().any(|item|item.place!=bace_inventory::ItemPlace::World || self.world.body(item.id).is_err()){return Err(G::Geometry);}
            }
        }
        let intent = request.intent.clone();
        let parent_location = self
            .generators
            .machines
            .get(&key.generator.entity)
            .ok_or(G::Stale)?
            .definition()
            .location;
        let placements: Vec<_> = items
            .iter()
            .map(|item| {
                let location = if let Ok((cell, state)) = self.world.actor_state(item.id) {
                    let p = state.position();
                    let angle = state.heading_radians() * 0.5;
                    bace_gameplay_api::GeneratorLocation {
                        cell: cell.0,
                        origin: [p.x, p.y, p.z],
                        rotation: [0.0, 0.0, angle.sin(), angle.cos()],
                    }
                } else {
                    parent_location
                };
                (item.id, item.template, location)
            })
            .collect();
        let nested = self.prepare_nested_generators(&intent, &placements)?;
        let public: Vec<_> = placements
            .iter()
            .filter(|(id, _, _)| self.world.contains_identity(*id))
            .copied()
            .collect();
        if public.len() > self.generators.capacity - self.generators.events.len() {
            return Err(G::Capacity);
        }
        let mut world_events = Vec::with_capacity(public.len());
        for (entity, template, location) in public {
            world_events.push(crate::GeneratorWorldEvent::Spawned {
                birth: match births {
                    Some(births) => births.get(&entity).cloned().ok_or(G::Invalid)?,
                    None => self.prepare_generator_birth(entity)?,
                },
                key,
                entity,
                template,
                location,
            });
        }
        let mut next = self.inventory.clone();
        next.register_generated(items, containers)
            .map_err(|error| {
                if error == E::Capacity {
                    G::Capacity
                } else {
                    G::Invalid
                }
            })?;
        self.confirm_generator_spawn(GeneratorSpawnReceipt {
            key,
            result: GeneratorSpawnResult::Completed {
                members: items
                    .iter()
                    .map(|item| GeneratorSpawnMember {
                        entity: item.id,
                        contribution: 1,
                    })
                    .collect(),
                materialized: true,
                failed_placements: 0,
            },
        })?;
        self.inventory = next;
        self.adopt_nested_generators(nested);
        self.generators.events.extend(world_events);
        Ok(())
    }
    pub fn admit_generated_world_inventory(
        &mut self,
        key: GeneratorSpawnKey,
        items: &[bace_inventory::InventoryItem],
        containers: &[bace_inventory::InventoryContainer],
        shapes: &[std::sync::Arc<bace_physics::CollisionShape>],
    ) -> Result<(), G> {
        if items.len() != shapes.len() || items.len() > 1024 {
            return Err(G::Invalid);
        }
        let mut inserted = Vec::with_capacity(items.len());
        let mut births = super::generator_births::Births::new();
        let result = (|| {
            for (item, shape) in items.iter().zip(shapes) {
                let actor = self.prepare_generated_world_actor(key, item.id, shape.clone())?;
                let birth = self.prepare_staged_generator_birth(&actor)?;
                self.world.insert(actor).map_err(|_| G::Placement)?;
                births.insert(item.id, birth);
                inserted.push(item.id);
            }
            self.admit_generated_inventory_with_births(key, items, containers, Some(&births))
        })();
        if result.is_err() {
            for id in inserted {
                self.world.remove(id);
            }
        }
        result
    }
    /// Only the adapter's exact world-epoch placement receipt can call this
    /// composite. Generic inventory completion rejects transient participants.
    pub fn confirm_generated_inventory_committed(
        &mut self,
        receipt: &crate::InventoryReceipt,
        transient: &[EntityId],
    ) -> Result<crate::InventoryTicket, E> {
        if self.inventory_commands.owns(receipt.operation)
            || self.generated_retirements.contains_key(&receipt.operation)
        {
            return Err(E::DurabilityPending);
        }
        let mut expected = self.inventory.generated_changes(receipt.operation)?;
        expected.sort_unstable();
        let mut supplied = transient.to_vec();
        supplied.sort_unstable();
        if expected.is_empty() || expected != supplied {
            return Err(E::InvalidState);
        }
        let accepted = self.confirm_inventory_committed_inner(receipt)?;
        for id in &expected {
            if self.inventory.owned(accepted.actor, *id) || self.inventory.item(*id).is_none() {
                self.world.remove(*id);
            }
        }
        self.inventory.adopt_generated_durability(&expected);
        Ok(accepted)
    }
    pub(super) fn prepare_generated_inventory_transition(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<Option<GeneratedInventoryTransition>, E> {
        let ticket = self
            .inventory
            .pending_ticket(receipt.operation)
            .ok_or(E::InvalidState)?;
        let ids: Vec<_> = ticket
            .proposal
            .changes
            .iter()
            .map(|c| c.after.id)
            .filter(|id| {
                self.generators
                    .machines
                    .values()
                    .any(|m| m.owns_member(*id))
            })
            .collect();
        if ids.is_empty() {
            return Ok(None);
        }
        let mut preview = self.inventory.clone();
        let ticket = preview.confirm(receipt)?;
        let mut updates = std::collections::BTreeMap::new();
        let mut effects = Vec::new();
        let mut removed = Vec::new();
        for id in ids {
            let notification = if preview.owned(ticket.actor, id) {
                GeneratorNotification::PickUp
            } else if preview.item(id).is_none() {
                GeneratorNotification::Destruction
            } else {
                continue;
            };
            let (&owner, machine) = self
                .generators
                .machines
                .iter()
                .find(|(_, m)| m.owns_member(id))
                .ok_or(E::InvalidState)?;
            let machine = updates.entry(owner).or_insert_with(|| machine.clone());
            let unix_seconds = self
                .generators
                .epoch
                .checked_add(i64::try_from(self.tick / 30).map_err(|_| E::Overflow)?)
                .ok_or(E::Overflow)?;
            let event = if let Some(name) = machine.definition().event.as_deref() {
                self.npcs
                    .generator_event(name, i32::try_from(unix_seconds).map_err(|_| E::Overflow)?)
                    .map_err(|_| E::InvalidState)?
            } else {
                GeneratorEventState::Missing
            };
            let clock = GeneratorClock {
                tick: self.tick,
                unix_seconds,
                is_day: self.generators.is_day,
                event,
            };
            effects.extend(
                machine
                    .notify(id, notification, clock)
                    .map_err(|_| E::InvalidState)?
                    .effects,
            );
            removed.push(id);
        }
        if effects.len() > self.generators.capacity - self.generators.effects.len() {
            return Err(E::Capacity);
        }
        Ok(Some(GeneratedInventoryTransition {
            updates,
            effects,
            removed,
        }))
    }
    pub(super) fn adopt_generated_inventory_transition(
        &mut self,
        transition: Option<GeneratedInventoryTransition>,
    ) {
        if let Some(transition) = transition {
            for id in transition.removed {
                self.world.remove(id);
            }
            self.generators.machines.extend(transition.updates);
            self.generators.effects.extend(transition.effects);
        }
    }
}
pub(super) struct GeneratedInventoryTransition {
    updates: std::collections::BTreeMap<EntityId, bace_spawning::GeneratorMachine>,
    effects: Vec<bace_gameplay_api::GeneratorLifecycleEffect>,
    removed: Vec<EntityId>,
}
