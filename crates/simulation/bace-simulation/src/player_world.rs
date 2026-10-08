//! Immutable accepted-world save projection; no body or mutable physics is copied.
use bace_entity::VitalPool;
use bace_geometry::Vec3;
use bace_types::{CellId, EntityId};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerWorldSnapshot {
    pub cell: CellId,
    pub position: Vec3,
    pub heading: f32,
    /// Health, stamina, mana. Missing pools are allowed only by explicit fixtures;
    /// production admission prepares all three.
    pub vitals: [Option<VitalPool>; 3],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerWorldSaveMarker {
    pub actor: EntityId,
    pub revision: u64,
    pub snapshot: PlayerWorldSnapshot,
}
pub(crate) struct WorldDirty {
    pub first_tick: u64,
    pub needs_revision: bool,
    pub in_flight: Option<(PlayerWorldSaveMarker, Option<u64>)>,
}
