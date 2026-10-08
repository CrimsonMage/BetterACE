//! Immutable ACE generator boundaries. Positions are requested placements, never
//! accepted physics state. Quaternion arrays use x, y, z, w order.
use bace_types::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GeneratorIdentity {
    pub entity: EntityId,
    pub incarnation: u64,
    pub content_revision: u64,
    pub random_identity: [u8; 16],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneratorLocation {
    pub cell: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GeneratorPositionSpec {
    pub cell: Option<u32>,
    pub origin: [Option<f32>; 3],
    pub rotation: [Option<f32>; 4],
}
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratorProfile {
    /// Source row order is preserved; linked profiles use their static GUID.
    pub id: u32,
    pub probability: f32,
    pub weenie_class_id: u32,
    pub delay: Option<f32>,
    pub init_create: i32,
    pub max_create: i32,
    pub when_create: u32,
    pub where_create: u32,
    pub stack_size: Option<i32>,
    pub palette_id: Option<u32>,
    pub shade: Option<f32>,
    pub position: GeneratorPositionSpec,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorKind {
    Object,
    Container,
    Creature,
    Vendor,
    Chest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorRotationType {
    Undefined,
    Relative,
    Absolute,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorTimeType {
    Undefined,
    RealTime,
    Defined,
    Event,
    Night,
    Day,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorDestruction {
    Nothing,
    Destroy,
    Kill,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratorDefinition {
    pub identity: GeneratorIdentity,
    pub profiles: Vec<GeneratorProfile>,
    pub location: GeneratorLocation,
    pub kind: GeneratorKind,
    pub initial_count: i32,
    pub maximum_count: i32,
    pub regeneration_interval: f64,
    pub initial_delay: f64,
    pub regeneration_timestamp: f64,
    pub time_type: GeneratorTimeType,
    pub event: Option<String>,
    pub start_time: i32,
    pub end_time: i32,
    pub disabled: bool,
    pub automatic_destruction: bool,
    pub parent: Option<EntityId>,
    pub destruction: GeneratorDestruction,
    pub end_destruction: GeneratorDestruction,
    pub rotation_type: GeneratorRotationType,
    pub use_rotation_offset: bool,
    pub radius: f32,
    pub vendor_shop_uses_generator: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneratorLink {
    pub profile_id: u32,
    pub weenie_class_id: u32,
    pub location: GeneratorLocation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorEventState {
    Missing,
    Available { enabled: bool, started: bool },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratorClock {
    /// Explicit monotonic 30 Hz simulation tick; no wall clock is read by domain.
    pub tick: u64,
    pub unix_seconds: i64,
    pub is_day: bool,
    pub event: GeneratorEventState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GeneratorSpawnKey {
    pub generator: GeneratorIdentity,
    pub profile_id: u32,
    pub occurrence: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeneratorDestination {
    Default(GeneratorLocation),
    Specific(GeneratorLocation),
    Scatter {
        center: GeneratorLocation,
        radius: f32,
        attempts: u32,
    },
    Contain {
        container: EntityId,
    },
    Shop {
        vendor: EntityId,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratorSpawnIntent {
    pub key: GeneratorSpawnKey,
    pub profile: GeneratorProfile,
    pub destination: GeneratorDestination,
    pub first_spawn: bool,
    pub due_tick: u64,
    /// Reusable event identity for all materialization/placement retries.
    pub random_identity: [u8; 16],
    pub random_key_version: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratorSpawnMember {
    pub entity: EntityId,
    /// Shop merges refer to the retained stock identity and contribution only.
    pub contribution: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorBlockedReason {
    Capacity,
    Content,
    Geometry,
    Inventory,
    Durability,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorFailure {
    MissingTemplate,
    MissingTreasure,
    InvalidDestination,
    InvalidContent,
    Placement,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeneratorSpawnResult {
    Blocked(GeneratorBlockedReason),
    Invalid(GeneratorFailure),
    Completed {
        members: Vec<GeneratorSpawnMember>,
        /// True when at least one object was materialized, before placement.
        materialized: bool,
        /// Permanent placement failures; first attempts suppress their slots.
        failed_placements: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratorSpawnReceipt {
    pub key: GeneratorSpawnKey,
    pub result: GeneratorSpawnResult,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorNotification {
    Destruction,
    PickUp,
    Death,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeneratorLifecycleEffect {
    DestroyMember {
        generator: GeneratorIdentity,
        member: GeneratorSpawnMember,
        recursive: bool,
        include_dead: bool,
        from_unload: bool,
    },
    KillMember {
        generator: GeneratorIdentity,
        member: GeneratorSpawnMember,
    },
    DetachMember {
        generator: GeneratorIdentity,
        entity: EntityId,
    },
    DestroySelf(GeneratorIdentity),
    SuppressedInitial {
        key: GeneratorSpawnKey,
        count: u32,
    },
    Invalidated {
        key: GeneratorSpawnKey,
        failure: GeneratorFailure,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GeneratorTransition {
    pub effects: Vec<GeneratorLifecycleEffect>,
    pub enqueued: usize,
    pub selection_attempts: u32,
    pub exhausted_initial_loop: bool,
}
