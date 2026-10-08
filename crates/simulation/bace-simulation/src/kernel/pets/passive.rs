//! Passive Pet has its own body/follow owner and never registers combat state.
//! PetDevice inventory changes still use the exact pet valuable operation.
use super::*;
use crate::PreparedPassivePet;
use crate::pets::{ActivePassivePet, PendingPassivePet};

impl Kernel {
    pub fn propose_passive_pet(
        &mut self,
        owner: EntityId,
        device: EntityId,
        pet: EntityId,
        profile: Arc<PreparedPassivePet>,
    ) -> Result<u64, PetError> {
        if profile.template == 0
            || pet.0 < 0x8000_0000
            || self.world.contains_identity(pet)
            || self.population.reserves_identity(pet)
            || self.generator_reserves_identity(pet)
            || self.magic.reserves_identity(pet)
            || self.pets.reserves_identity(pet)
        {
            return Err(PetError::InvalidProfile);
        }
        if self.pets.pending.len()
            + self.pets.pending_passive.len()
            + self.pets.active.len()
            + self.pets.active_passive.len()
            >= self.pets.capacity
            || self.pets.provenance.len() >= self.pets.capacity
        {
            return Err(PetError::Capacity);
        }
        if self.pets.reserved(owner)
            || self.characters.reserved(owner)
            || self.inventory.reserved(owner)
            || self.pets.owner_has_active(owner)
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
        user.active_combat_pet = false;
        user.cooldown_active = false;
        if self.world.combatant(owner).is_none_or(|c| c.health() == 0) {
            return Err(PetError::MissingOwner);
        }
        profile.requirements.check(user).map_err(PetError::Use)?;
        let (cell, state) = self
            .world
            .actor_state(owner)
            .map_err(|_| PetError::MissingOwner)?;
        let radius = self
            .world
            .body(owner)
            .map_err(|_| PetError::MissingOwner)?
            .collision_radius();
        let distance = radius + profile.shape.horizontal_radius() + 2.0;
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
        self.pets.pending_passive.insert(
            operation,
            PendingPassivePet {
                actor,
                owner,
                device,
                committed: false,
            },
        );
        Ok(operation)
    }

    pub(super) fn service_passive_pets(&mut self) {
        let ready: Vec<_> = self
            .pets
            .pending_passive
            .iter()
            .filter(|(_, pending)| pending.committed)
            .map(|(&operation, _)| operation)
            .take(32)
            .collect();
        for operation in ready {
            if self.pets.events.len() >= self.pets.capacity {
                break;
            }
            let pending = &self.pets.pending_passive[&operation];
            if self.world.validate_actor(&pending.actor).is_err() {
                continue;
            }
            let pending = self
                .pets
                .pending_passive
                .remove(&operation)
                .expect("selected passive pet");
            let id = pending.actor.id;
            if let Err((_error, actor)) =
                self.world.insert_damage_owned(pending.actor, pending.owner)
            {
                self.pets.pending_passive.insert(
                    operation,
                    PendingPassivePet {
                        actor: *actor,
                        ..pending
                    },
                );
                continue;
            }
            self.pets.provenance.insert(id, pending.owner);
            self.pets.active_passive.insert(
                id,
                ActivePassivePet {
                    owner: pending.owner,
                    device: pending.device,
                    sequence: 0,
                },
            );
            self.pets.events.push_back(PetEvent::Spawned {
                pet: id,
                owner: pending.owner,
                device: pending.device,
            });
        }
        let ids: Vec<_> = self
            .pets
            .active_passive
            .keys()
            .copied()
            .take(4096)
            .collect();
        for id in ids {
            if self.pets.retiring.values().any(|&pet| pet == id) {
                continue;
            }
            let active = &self.pets.active_passive[&id];
            let owner = active.owner;
            let device = active.device;
            let states = self
                .world
                .actor_state(id)
                .ok()
                .zip(self.world.actor_state(owner).ok());
            let should_stow = self
                .world
                .combatant(owner)
                .is_none_or(|combatant| combatant.health() == 0)
                || states
                    .as_ref()
                    .is_none_or(|((pet_cell, pet), (owner_cell, owner_state))| {
                        pet_cell != owner_cell
                            || (pet.position() - owner_state.position()).length_squared()
                                > 192.0 * 192.0
                    });
            if should_stow {
                let _ = self.reserve_passive_release(owner, device, id);
                continue;
            }
            let Some(((_, pet), (_, owner_state))) = states else {
                continue;
            };
            let delta = owner_state.position() - pet.position();
            let direction = if delta.length_squared() > 2.0 * 2.0 {
                delta.horizontal_clamped()
            } else {
                Vec3::ZERO
            };
            if let Some(active) = self.pets.active_passive.get_mut(&id)
                && let Some(sequence) = active.sequence.checked_add(1)
                && let Ok(body) = self.world.body_mut(id)
                && let Ok(intent) = bace_motion::MotionIntent::new(direction, false)
            {
                active.sequence = sequence;
                let _ = body.submit_intent(body.accepted().epoch(), sequence, intent);
            }
        }
    }

    pub(super) fn reserve_passive_release(
        &mut self,
        owner: EntityId,
        device: EntityId,
        pet: EntityId,
    ) -> Result<u64, PetError> {
        if let Some((&operation, _)) = self.pets.retiring.iter().find(|(_, id)| **id == pet) {
            return Ok(operation);
        }
        if self.pets.events.len() >= self.pets.capacity {
            return Err(PetError::Capacity);
        }
        let items: Vec<_> = self.inventory.items().cloned().collect();
        let containers: Vec<_> = self.inventory.containers().copied().collect();
        let proposal = propose_pet_release(
            owner,
            device,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )
        .map_err(|_| PetError::Durability)?;
        let operation = self
            .stage_inventory(owner, |inventory| inventory.reserve(owner, proposal))
            .map_err(|_| PetError::Durability)?;
        let Some(ticket) = self.inventory.pending_ticket(operation).cloned() else {
            let _ = self.reject_inventory_inner(operation);
            return Err(PetError::Durability);
        };
        if self.inventory.claim(operation).is_err() {
            let _ = self.reject_inventory_inner(operation);
            return Err(PetError::Durability);
        }
        self.pets.retiring.insert(operation, pet);
        let actor_revision = self
            .characters
            .get(owner)
            .map_or(0, |character| character.revision());
        self.pets.events.push_back(PetEvent::ReleaseProposed {
            ticket,
            actor_revision,
        });
        Ok(operation)
    }
}
