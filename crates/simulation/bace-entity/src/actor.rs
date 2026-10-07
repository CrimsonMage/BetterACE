use bace_physics::Body;
use bace_types::{CellId, EntityId};

/// Owned by World. Replication should consume AcceptedState, never this mutable
/// aggregate. Body itself exposes no client-controlled accepted-pose setter.
pub struct Actor {
    pub id: EntityId,
    pub cell: CellId,
    pub body: Body,
}
