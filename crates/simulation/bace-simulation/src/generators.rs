//! Generator ownership and bounded adapter lanes. Spawned NPCs share Population;
//! placement/item/vendor work never creates another mutable entity registry.
use bace_gameplay_api::generators::*;
use bace_random::RandomRoot;
use bace_spawning::{GeneratorError, GeneratorMachine};
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

#[derive(Clone)]
pub struct GeneratedNpcTemplate {
    pub script: Option<crate::PreparedNpcScriptSource>,
    pub combat_assets: Option<Arc<crate::PreparedNpcCombatAssets>>,
    pub death_motions: Vec<bace_motion::PreparedDeathMotion>,
    pub physical_motions: Vec<(u32, f32, Arc<bace_motion::PreparedMotionChain>)>,
    pub locomotion_styles: Vec<Arc<bace_motion::AnimatedLocomotion>>,
    pub requires_equipment: bool,
    pub resources: [bace_entity::VitalPool; 2],
    pub missile_timings: Vec<PreparedNpcMissileTiming>,
    pub missile_timing_errors: Vec<(u32, String)>,
    pub blueprint: crate::pve::NpcBlueprint,
    pub geometry: crate::pve::PreparedNpcGeometry,
    pub physical: Option<Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>>,
    pub loot: Option<crate::pve::NativeLootPolicy>,
    pub ace_loot: Option<crate::pve::AceCreatureLootPolicy>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratorHostRequest {
    /// Source generator region, including Contain/Shop and nested dynamic roots.
    pub landblock: u16,
    /// Accepted main/pack counts at delivery; retries refresh placement only.
    pub next_slots: Option<(u32, u32)>,
    pub intent: GeneratorSpawnIntent,
    pub entities: Vec<EntityId>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum GeneratorWorldEvent {
    Spawned {
        birth: Option<Arc<bace_gameplay_api::visibility::AcceptedObjectBirth>>,
        key: GeneratorSpawnKey,
        entity: EntityId,
        template: u32,
        location: GeneratorLocation,
    },
    Lifecycle(GeneratorLifecycleEffect),
    Blocked {
        key: GeneratorSpawnKey,
        reason: GeneratorBlockedReason,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorServiceError {
    Capacity,
    Invalid,
    GeneratedItemForest {
        stage: &'static str,
        entity: EntityId,
        slot: Option<u32>,
        inventory: Option<bace_gameplay_api::InventoryRejection>,
    },
    Stale,
    Busy,
    Missing,
    Geometry,
    Placement,
    Domain(GeneratorError),
}
impl From<GeneratorError> for GeneratorServiceError {
    fn from(value: GeneratorError) -> Self {
        Self::Domain(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorControl {
    Generate,
    Activate,
    Reset,
    Regenerate,
    Death,
    Destroy,
    Unload,
}
pub(crate) struct Generators {
    pub(crate) prepared_births: bool,
    pub(crate) machines: BTreeMap<EntityId, GeneratorMachine>,
    pub(crate) templates: BTreeMap<(u64, u32), GeneratedNpcTemplate>,
    pub(crate) definitions: BTreeMap<(u64, u32), Arc<GeneratorDefinition>>,
    pub(crate) requests: BTreeMap<GeneratorSpawnKey, GeneratorHostRequest>,
    pub(crate) cursors: BTreeMap<EntityId, GeneratorSpawnKey>,
    pub(crate) submitted: std::collections::BTreeSet<GeneratorSpawnKey>,
    pub(crate) ids: VecDeque<EntityId>,
    pub(crate) events: VecDeque<GeneratorWorldEvent>,
    pub(crate) effects: VecDeque<GeneratorLifecycleEffect>,
    pub(crate) lifecycle_frames: BTreeMap<EntityId, VecDeque<GeneratorLifecycleEffect>>,
    pub(crate) root: Option<Arc<RandomRoot>>,
    pub(crate) epoch: i64,
    pub(crate) is_day: bool,
    pub(crate) capacity: usize,
    pub(crate) scratch: Vec<EntityId>,
    pub(crate) activation_fences: BTreeMap<u16, (u64, u64)>,
}
impl Generators {
    pub(crate) fn new(capacity: usize) -> Self {
        let capacity = capacity.min(4096);
        Self {
            prepared_births: false,
            machines: BTreeMap::new(),
            templates: BTreeMap::new(),
            definitions: BTreeMap::new(),
            requests: BTreeMap::new(),
            cursors: BTreeMap::new(),
            submitted: Default::default(),
            ids: VecDeque::with_capacity(capacity),
            events: VecDeque::with_capacity(capacity),
            effects: VecDeque::with_capacity(capacity),
            lifecycle_frames: BTreeMap::new(),
            root: None,
            epoch: 0,
            is_day: true,
            capacity,
            scratch: Vec::with_capacity(capacity),
            activation_fences: BTreeMap::new(),
        }
    }
    pub(crate) fn reserves(&self, id: EntityId) -> bool {
        self.ids.contains(&id)
            || self.requests.values().any(|r| r.entities.contains(&id))
            || self.machines.contains_key(&id)
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.ids.is_empty()
            || !self.machines.is_empty()
            || !self.requests.is_empty()
            || !self.effects.is_empty()
            || !self.lifecycle_frames.is_empty()
            || !self.events.is_empty()
    }
    pub(crate) fn adopt(
        &mut self,
        id: EntityId,
        machine: GeneratorMachine,
        transition: GeneratorTransition,
    ) -> Result<(), GeneratorServiceError> {
        if transition.effects.len() > self.capacity - self.effects.len() {
            return Err(GeneratorServiceError::Capacity);
        }
        self.effects.extend(transition.effects);
        self.machines.insert(id, machine);
        Ok(())
    }
}

/// Immutable cold-region input. Bodies are created only by the owner against
/// this exact geometry generation; no supplied pose is accepted beforehand.
#[derive(Clone)]
pub struct PreparedGeneratorRoot {
    pub script: Option<crate::PreparedNpcScriptSource>,
    pub entity: EntityId,
    pub location: GeneratorLocation,
    pub shape: Arc<bace_physics::CollisionShape>,
    pub creature: Option<GeneratedNpcTemplate>,
    pub loadout: Option<PreparedNpcLoadout>,
}
#[derive(Clone)]
pub struct PreparedGeneratorRegion {
    pub landblock: u16,
    pub epoch: u64,
    pub revision: u64,
    pub geometry: Arc<bace_physics::GeometryRegion>,
    pub roots: Vec<PreparedGeneratorRoot>,
    pub containers: Vec<bace_inventory::InventoryContainer>,
    pub definitions: Vec<Arc<GeneratorDefinition>>,
    pub templates: Vec<(u32, Arc<GeneratorDefinition>)>,
    pub creatures: Vec<(u32, GeneratedNpcTemplate)>,
}

#[derive(Clone)]
pub struct PreparedNpcLoadout {
    pub combat_assets: Option<Arc<crate::PreparedNpcCombatAssets>>,
    pub death_motions: Vec<bace_motion::PreparedDeathMotion>,
    pub physical_motions: Vec<(u32, f32, Arc<bace_motion::PreparedMotionChain>)>,
    pub locomotion_styles: Vec<Arc<bace_motion::AnimatedLocomotion>>,
    pub items: Vec<bace_inventory::InventoryItem>,
    pub containers: Vec<bace_inventory::InventoryContainer>,
    pub profile: Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
    pub death_items: Vec<bace_content::WeenieV1>,
    pub death_parents: Vec<Option<usize>>,
    pub death_ids: Vec<EntityId>,
    pub enchantments: Vec<crate::PreparedGeneratorEnchantment>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedNpcMissileTiming {
    pub style: u32,
    pub launch_seconds: f64,
    pub reload_seconds: f64,
    pub return_seconds: f64,
}
