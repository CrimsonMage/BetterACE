//! Single-owner pets. World owns every accepted body; durable device
//! operations gate publication and retirement. Retained ownership is bounded.
use bace_ai::{CombatPetLifetime, PetUseRequirements, PetUser};
use bace_entity::{Actor, Combatant, CombatantProfile};
use bace_motion::Capabilities;
use bace_physics::CollisionShape;
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
#[derive(Clone, Debug)]
pub struct PreparedCombatPet {
    pub template: u32,
    pub shape: Arc<CollisionShape>,
    pub capabilities: Capabilities,
    pub combat: CombatantProfile,
    pub lifetime_seconds: f64,
    pub visual_range: f32,
    pub requirements: PetUseRequirements,
    pub cooldown_group: Option<u16>,
    pub cooldown_seconds: f64,
}
#[derive(Clone, Debug)]
pub struct PreparedPassivePet {
    pub template: u32,
    pub shape: Arc<CollisionShape>,
    pub capabilities: Capabilities,
    pub requirements: PetUseRequirements,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PetError {
    Unauthorized,
    MissingOwner,
    MissingDevice,
    InvalidProfile,
    Requirements,
    Use(bace_ai::PetUseError),
    ActivePet,
    Capacity,
    Geometry,
    Reserved,
    Durability,
    Activation(bace_inventory::ActivationFailure),
    UnknownOperation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PetEvent {
    ReleaseProposed {
        ticket: crate::InventoryTicket,
        actor_revision: u64,
    },
    Spawned {
        pet: EntityId,
        owner: EntityId,
        device: EntityId,
    },
    Despawned {
        pet: EntityId,
        owner: EntityId,
    },
    Rejected {
        operation: u64,
        owner: EntityId,
    },
}
pub(crate) struct PendingPet {
    pub actor: Actor,
    pub combatant: Combatant,
    pub owner: EntityId,
    pub device: EntityId,
    pub profile: Arc<PreparedCombatPet>,
    pub committed: bool,
    pub cooldown: Option<bace_magic::PreparedEnchantment>,
    pub registry_after: Vec<bace_magic::EnchantmentEntry>,
    pub registry_revision: u64,
}
pub(crate) struct ActivePet {
    pub lifetime: CombatPetLifetime,
    pub profile: Arc<PreparedCombatPet>,
    pub next_attack: f64,
    pub sequence: u32,
}
pub(crate) struct PendingPassivePet {
    pub actor: Actor,
    pub owner: EntityId,
    pub device: EntityId,
    pub committed: bool,
}
pub(crate) struct ActivePassivePet {
    pub owner: EntityId,
    pub device: EntityId,
    pub sequence: u32,
}
pub(crate) struct Pets {
    pub pending: BTreeMap<u64, PendingPet>,
    pub pending_passive: BTreeMap<u64, PendingPassivePet>,
    pub active: BTreeMap<EntityId, ActivePet>,
    pub active_passive: BTreeMap<EntityId, ActivePassivePet>,
    pub provenance: BTreeMap<EntityId, EntityId>,
    pub owners: BTreeMap<EntityId, PetUser>,
    pub retiring: BTreeMap<u64, EntityId>,
    pub committed_retirements: std::collections::BTreeSet<u64>,
    pub events: VecDeque<PetEvent>,
    pub capacity: usize,
}
impl Pets {
    pub(crate) fn owner_has_active(&self, owner: EntityId) -> bool {
        self.active.values().any(|p| p.lifetime.owner == owner.0)
            || self.active_passive.values().any(|p| p.owner == owner)
    }

    pub(crate) fn new(capacity: usize) -> Self {
        let capacity = capacity.min(4096);
        Self {
            pending: BTreeMap::new(),
            pending_passive: BTreeMap::new(),
            active: BTreeMap::new(),
            active_passive: BTreeMap::new(),
            provenance: BTreeMap::new(),
            owners: BTreeMap::new(),
            retiring: BTreeMap::new(),
            committed_retirements: Default::default(),
            events: VecDeque::with_capacity(capacity),
            capacity,
        }
    }
    pub(crate) fn reserved(&self, owner: EntityId) -> bool {
        self.pending.values().any(|p| p.owner == owner)
            || self.pending_passive.values().any(|p| p.owner == owner)
            || self
                .retiring
                .values()
                .any(|id| self.provenance.get(id) == Some(&owner))
    }
    pub(crate) fn owner(&self, id: EntityId) -> Option<EntityId> {
        self.provenance.get(&id).copied()
    }
    pub(crate) fn reserves_identity(&self, id: EntityId) -> bool {
        self.provenance.contains_key(&id)
            || self.pending.values().any(|p| p.actor.id == id)
            || self.pending_passive.values().any(|p| p.actor.id == id)
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.pending.is_empty()
            || !self.pending_passive.is_empty()
            || !self.active.is_empty()
            || !self.active_passive.is_empty()
            || !self.retiring.is_empty()
            || !self.committed_retirements.is_empty()
            || !self.events.is_empty()
    }
}
