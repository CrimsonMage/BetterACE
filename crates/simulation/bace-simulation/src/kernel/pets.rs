//! Combat-pet publication shares the world's owner and exact inventory receipts.
use super::*;
use crate::pets::{ActivePet, PendingPet};
use crate::{PetError, PetEvent, PreparedCombatPet};
use bace_ai::{CombatPetLifetime, PetTarget, PetUser, combat_pet_target};
use bace_entity::{Actor, Combatant};
use bace_geometry::Vec3;
use bace_inventory::{InventoryView, propose_pet_charge, propose_pet_release};
use std::sync::Arc;
mod passive;
impl Kernel {
    /// Rebuilt from authoritative progression/augmentation properties after load
    /// and relevant training changes. Request packets cannot supply this profile.
    pub fn register_pet_owner(&mut self, owner: EntityId, user: PetUser) -> Result<(), PetError> {
        if self.pets.reserved(owner) {
            return Err(PetError::Reserved);
        }
        if self.characters.get(owner).is_none() {
            return Err(PetError::MissingOwner);
        }
        if self.pets.owners.len() >= self.pets.capacity && !self.pets.owners.contains_key(&owner) {
            return Err(PetError::Capacity);
        }
        self.pets.owners.insert(owner, user);
        Ok(())
    }
    pub fn pending_pet_registry(
        &self,
        operation: u64,
    ) -> Option<(EntityId, u64, &[bace_magic::EnchantmentEntry])> {
        let pending = self.pets.pending.get(&operation)?;
        Some((
            pending.owner,
            pending.registry_revision,
            &pending.registry_after,
        ))
    }
    pub fn pet_owner(&self, pet: EntityId) -> Option<EntityId> {
        self.pets.owner(pet)
    }
    pub fn take_pet_event(&mut self) -> Option<PetEvent> {
        self.pets.events.pop_front()
    }
    pub fn has_pet_state(&self) -> bool {
        self.pets.has_state()
    }
    pub fn propose_combat_pet(
        &mut self,
        owner: EntityId,
        device: EntityId,
        pet: EntityId,
        profile: Arc<PreparedCombatPet>,
    ) -> Result<u64, PetError> {
        let now = self.tick as f64 / 30.0;
        if pet.0 == 0
            || self.world.contains_identity(pet)
            || (self.population.reserves_identity(pet) || self.generator_reserves_identity(pet))
            || self.magic.reserves_identity(pet)
            || self.pets.reserves_identity(pet)
        {
            return Err(PetError::InvalidProfile);
        }
        if self.pets.pending.len()
            + self.pets.active.len()
            + self.pets.pending_passive.len()
            + self.pets.active_passive.len()
            >= self.pets.capacity
            || self.pets.provenance.len() >= self.pets.capacity
        {
            return Err(PetError::Capacity);
        }
        if self.pets.reserved(owner)
            || self.characters.reserved(owner)
            || self.inventory.reserved(owner)
        {
            return Err(PetError::Reserved);
        }
        let mut user = *self.pets.owners.get(&owner).ok_or(PetError::MissingOwner)?;
        if let Some(skills) = self.combat.skills.get(&owner) {
            let summoning = skills.skill(54);
            user.advancement = summoning.advancement as u32;
            user.skill = summoning.current;
        }
        if let Some(services) = self.characters.native_services(owner) {
            user.level = services.level;
        }
        user.portal_space |= self.portals.reserved(owner);
        let item = self.inventory.item(device).ok_or(PetError::MissingDevice)?;
        user.owns_device = self.inventory.owned(owner, device);
        user.charges = item.structure.unwrap_or(0);
        user.active_combat_pet = self.pets.owner_has_active(owner);
        user.cooldown_active = profile.cooldown_group.is_some_and(|group| {
            self.magic.registry(owner).is_some_and(|r| {
                r.entries().iter().any(|e| {
                    e.spell == (0x8000 | u32::from(group)) && e.spec.duration + e.start_time != 0.0
                })
            })
        });
        if self.world.combatant(owner).is_none_or(|c| c.health() == 0) {
            return Err(PetError::MissingOwner);
        }
        profile.requirements.check(user).map_err(PetError::Use)?;
        if profile.template == 0
            || profile.combat.player
            || !profile.visual_range.is_finite()
            || !(0.0..=192.0).contains(&profile.visual_range)
            || !profile.cooldown_seconds.is_finite()
            || !(0.0..=86400.0).contains(&profile.cooldown_seconds)
        {
            return Err(PetError::InvalidProfile);
        }
        CombatPetLifetime::new(owner.0, device.0, profile.lifetime_seconds, now)
            .map_err(|_| PetError::InvalidProfile)?;
        let (cell, state) = self
            .world
            .actor_state(owner)
            .map_err(|_| PetError::MissingOwner)?;
        let radius = self
            .world
            .body(owner)
            .map_err(|_| PetError::MissingOwner)?
            .collision_radius();
        let distance = radius + profile.shape.horizontal_radius() + 0.25;
        let (sin, cos) = state.heading_radians().sin_cos();
        let body = self
            .world
            .prepare_geometry_body(bace_physics::GeometrySpawn {
                cell: cell.0,
                position: state.position() + Vec3::new(-sin * distance, cos * distance, 0.0),
                shape: profile.shape.clone(),
                capabilities: profile.capabilities,
                heading: state.heading_radians(),
                maximum_turn_rate: std::f32::consts::PI,
            })
            .map_err(|_| PetError::Geometry)?;
        let mut combatant =
            Combatant::new(profile.combat.clone()).map_err(|_| PetError::InvalidProfile)?;
        combatant.set_mode(2);
        let actor = Actor {
            id: pet,
            cell,
            body,
        };
        self.world
            .validate_actor(&actor)
            .map_err(|_| PetError::Geometry)?;
        self.prepare_inventory_time()
            .map_err(|_| PetError::Reserved)?;
        let registry = self.magic.registry(owner).ok_or(PetError::MissingOwner)?;
        let cooldown = if let Some(group) = profile.cooldown_group {
            Some(
                registry
                    .propose_cooldown(group, device.0, profile.cooldown_seconds)
                    .map_err(|_| PetError::Reserved)?,
            )
        } else {
            None
        };
        let registry_after = if let Some(ref proposal) = cooldown {
            registry.preview(proposal).map_err(|_| PetError::Reserved)?
        } else {
            registry.entries().to_vec()
        };
        let registry_revision = registry
            .revision()
            .checked_add(u64::from(cooldown.is_some()))
            .ok_or(PetError::Capacity)?;
        let items: Vec<_> = self.inventory.items().cloned().collect();
        let containers: Vec<_> = self.inventory.containers().copied().collect();
        let proposal = propose_pet_charge(
            owner,
            device,
            profile.requirements.unlimited,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )
        .map_err(|_| PetError::Requirements)?;
        let operation = self
            .stage_inventory(owner, |inventory| inventory.reserve(owner, proposal))
            .map_err(|_| PetError::Durability)?;
        self.pets.pending.insert(
            operation,
            PendingPet {
                actor,
                combatant,
                owner,
                device,
                profile,
                committed: false,
                cooldown,
                registry_after,
                registry_revision,
            },
        );
        Ok(operation)
    }
    pub(super) fn finish_pet_operation(&mut self, operation: u64, success: bool) {
        if let Some(pending) = self.pets.pending.get_mut(&operation) {
            if success {
                if let Some(cooldown) = pending.cooldown.take() {
                    self.magic.adopt_pet_cooldown(pending.owner, cooldown);
                }
                pending.committed = true;
            } else {
                self.pets.pending.remove(&operation);
            }
        }
        if let Some(pending) = self.pets.pending_passive.get_mut(&operation) {
            if success {
                pending.committed = true;
            } else {
                self.pets.pending_passive.remove(&operation);
            }
        }
        if success && self.pets.retiring.contains_key(&operation) {
            self.pets.committed_retirements.insert(operation);
            self.drain_committed_pet_retirements();
        }
        if !success {
            self.pets.retiring.remove(&operation);
            self.pets.committed_retirements.remove(&operation);
        }
    }
    fn drain_committed_pet_retirements(&mut self) {
        let ready: Vec<_> = self
            .pets
            .committed_retirements
            .iter()
            .copied()
            .take(32)
            .collect();
        for operation in ready {
            if self.pets.events.len() >= self.pets.capacity {
                break;
            }
            let Some(&pet) = self.pets.retiring.get(&operation) else {
                self.pets.committed_retirements.remove(&operation);
                continue;
            };
            let combat = self
                .pets
                .active
                .get(&pet)
                .map(|active| EntityId(active.lifetime.owner));
            let passive = self
                .pets
                .active_passive
                .get(&pet)
                .map(|active| active.owner);
            let Some(owner) = combat.or(passive) else {
                // The exact release is durable, but this owner must retain the
                // retirement until its active world body can be removed.
                continue;
            };
            // A committed release may have waited for output capacity. Recheck
            // participants before removing the body on every retained retry.
            if self.combat.physical_proc_pending(pet) || self.world.remove(pet).is_none() {
                continue;
            }
            self.combat.retire_actor(pet);
            self.pets.active.remove(&pet);
            self.pets.active_passive.remove(&pet);
            self.pets.provenance.remove(&pet);
            self.pets.retiring.remove(&operation);
            self.pets.committed_retirements.remove(&operation);
            self.pets
                .events
                .push_back(PetEvent::Despawned { pet, owner });
        }
    }
    pub(super) fn pet_receipt_ready(&self, operation: u64) -> bool {
        self.pets
            .retiring
            .get(&operation)
            .is_none_or(|pet| !self.combat.physical_proc_pending(*pet))
            && (!self.pets.retiring.contains_key(&operation)
                || self.pets.events.len() < self.pets.capacity)
            && (!self.pets.pending.contains_key(&operation) || self.magic.can_accept())
            && (!self.pets.pending_passive.contains_key(&operation)
                || self.pets.events.len() < self.pets.capacity)
    }
    pub(super) fn service_pets(&mut self) {
        self.drain_committed_pet_retirements();
        let now = self.tick as f64 / 30.0;
        let active = &self.pets.active;
        let world = &self.world;
        self.pets
            .provenance
            .retain(|id, _| active.contains_key(id) || world.contains_identity(*id));
        let ready: Vec<_> = self
            .pets
            .pending
            .iter()
            .filter(|(_, p)| p.committed)
            .map(|(&op, _)| op)
            .take(32)
            .collect();
        for operation in ready {
            if self.pets.events.len() >= self.pets.capacity {
                break;
            }
            let pending = &self.pets.pending[&operation];
            if self.world.validate_actor(&pending.actor).is_err() {
                continue;
            }
            let pending = self
                .pets
                .pending
                .remove(&operation)
                .expect("selected pending pet");
            let id = pending.actor.id;
            // Pure placement validation immediately preceded both insertions on
            // this same owner; no command or I/O can interleave.
            if let Err((_error, actor)) =
                self.world.insert_damage_owned(pending.actor, pending.owner)
            {
                self.pets.pending.insert(
                    operation,
                    PendingPet {
                        actor: *actor,
                        ..pending
                    },
                );
                continue;
            }
            self.world
                .register_combatant(id, pending.combatant)
                .expect("new pet identity");
            let lifetime = CombatPetLifetime::new(
                pending.owner.0,
                pending.device.0,
                pending.profile.lifetime_seconds,
                now,
            )
            .expect("prepared lifespan");
            self.pets.provenance.insert(id, pending.owner);
            self.pets.active.insert(
                id,
                ActivePet {
                    lifetime,
                    profile: pending.profile,
                    next_attack: now,
                    sequence: 0,
                },
            );
            self.pets.events.push_back(PetEvent::Spawned {
                pet: id,
                owner: pending.owner,
                device: pending.device,
            });
        }
        let ids: Vec<_> = self.pets.active.keys().copied().collect();
        for id in ids {
            if self.combat.physical_proc_pending(id)
                || self.pets.retiring.values().any(|pet| *pet == id)
            {
                continue;
            }
            let active = &self.pets.active[&id];
            let owner = EntityId(active.lifetime.owner);
            let device = EntityId(active.lifetime.device);
            let owner_alive = self.world.combatant(owner).is_some_and(|c| c.health() > 0);
            if active.lifetime.expired(now, owner_alive)
                || self.world.combatant(id).is_none_or(|c| c.health() == 0)
            {
                if self.pets.events.len() >= self.pets.capacity {
                    continue;
                }
                self.combat.cancel(id);
                let items: Vec<_> = self.inventory.items().cloned().collect();
                let containers: Vec<_> = self.inventory.containers().copied().collect();
                if let Ok(proposal) = propose_pet_release(
                    owner,
                    device,
                    InventoryView {
                        items: &items,
                        containers: &containers,
                    },
                ) && let Ok(operation) =
                    self.stage_inventory(owner, |inventory| inventory.reserve(owner, proposal))
                {
                    if let Some(ticket) = self.inventory.pending_ticket(operation).cloned()
                        && self.inventory.claim(operation).is_ok()
                    {
                        self.pets.retiring.insert(operation, id);
                        let actor_revision = self
                            .characters
                            .get(owner)
                            .map_or(0, |character| character.revision());
                        self.pets.events.push_back(PetEvent::ReleaseProposed {
                            ticket,
                            actor_revision,
                        });
                    } else {
                        let _ = self.reject_inventory_inner(operation);
                    }
                }
                continue;
            }
            if now < active.next_attack || self.combat.active(id) {
                continue;
            }
            let Ok((cell, state)) = self.world.actor_state(id) else {
                continue;
            };
            let candidates: Vec<_> = self
                .world
                .states()
                .filter(|(_, other, _)| *other == cell)
                .filter_map(|(candidate, _, position)| {
                    let combatant = self.world.combatant(candidate)?;
                    Some(PetTarget {
                        id: candidate.0,
                        creature: candidate != owner && candidate != id,
                        player: combatant.profile().player,
                        combat_pet: self.pets.owner(candidate).is_some(),
                        alive: combatant.health() > 0,
                        attackable: true,
                        visible: self
                            .world
                            .attack_geometry(id, candidate, active.profile.visual_range)
                            .is_ok_and(|(_, clear)| clear),
                        same_faction: false,
                        retaliating: false,
                        distance_squared: (position.position() - state.position()).length_squared(),
                    })
                })
                .take(4096)
                .collect();
            let target = combat_pet_target(&candidates, active.profile.visual_range)
                .ok()
                .flatten()
                .map(EntityId);
            let mut direction = Vec3::ZERO;
            if let Some(target) = target {
                let (range, clear) = self
                    .world
                    .attack_geometry(id, target, active.profile.combat.melee_range)
                    .unwrap_or((false, false));
                if !range || !clear {
                    if let Ok((_, target_state)) = self.world.actor_state(target) {
                        direction =
                            (target_state.position() - state.position()).horizontal_clamped();
                    }
                } else if self
                    .combat
                    .apply_with_equipment(
                        &mut self.world,
                        id,
                        bace_gameplay_api::CombatRequest::TargetedMelee {
                            target,
                            height: 2,
                            power: 0.5,
                        },
                        now,
                        &self.inventory,
                    )
                    .is_ok()
                {
                    self.pets
                        .active
                        .get_mut(&id)
                        .expect("active pet")
                        .next_attack = now + 2.5;
                }
            }
            if let Some(pet) = self.pets.active.get_mut(&id)
                && let Some(sequence) = pet.sequence.checked_add(1)
                && let Ok(body) = self.world.body_mut(id)
                && let Ok(intent) = bace_motion::MotionIntent::new(direction, false)
            {
                pet.sequence = sequence;
                let _ = body.submit_intent(body.accepted().epoch(), sequence, intent);
            }
        }
        self.service_passive_pets();
    }
}
impl Kernel {
    /// Logout/stow initiates the same durable device release as lifespan expiry.
    /// A pending uncertain summon must settle before this operation can proceed.
    pub fn request_pet_stow(&mut self, owner: EntityId) -> Result<Option<u64>, PetError> {
        if self.pets.pending.values().any(|p| p.owner == owner) {
            return Err(PetError::Durability);
        }
        if self.pets.pending_passive.values().any(|p| p.owner == owner) {
            return Err(PetError::Durability);
        }
        let Some(id) = self
            .pets
            .active
            .iter()
            .find(|(_, p)| p.lifetime.owner == owner.0)
            .map(|(&id, _)| id)
        else {
            if let Some((pet, device)) = self
                .pets
                .active_passive
                .iter()
                .find(|(_, active)| active.owner == owner)
                .map(|(&pet, active)| (pet, active.device))
            {
                return self.reserve_passive_release(owner, device, pet).map(Some);
            }
            return Ok(None);
        };
        if let Some((&operation, _)) = self.pets.retiring.iter().find(|(_, pet)| **pet == id) {
            return Ok(Some(operation));
        }
        self.pets
            .active
            .get_mut(&id)
            .expect("selected pet")
            .lifetime
            .expires = self.tick as f64 / 30.0;
        self.service_pets();
        self.pets
            .retiring
            .iter()
            .find(|(_, pet)| **pet == id)
            .map(|(&operation, _)| Some(operation))
            .ok_or(PetError::Durability)
    }
}
