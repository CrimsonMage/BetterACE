//! Retained accepted equipment appearance handoff to the visibility owner.
//! This replaces model/attachments only; accepted world pose stays authoritative.
use bace_types::EntityId;
use bace_wire::{ObjectDescription, ObjectModel, PhysicsChild};
use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct EquipmentVisibilityUpdate {
    pub actor: EntityId,
    pub operation: u64,
    pub before_revision: u64,
    pub after_revision: u64,
    pub incarnation: u64,
    pub instance_sequence: u16,
    pub model: ObjectModel,
    pub children: Vec<PhysicsChild>,
    pub descriptions: Vec<Arc<ObjectDescription>>,
}
