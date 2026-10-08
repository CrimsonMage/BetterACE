//! Entity ownership, cells, landblocks and spatial state.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod world;

pub use world::{CorpseState, DoorCollider, HealthObservation, World, WorldError};
pub use world::{PreparedCellVisibility, VisibilityError, visibility_distance_squared};
pub use world::{WorldNpcAdmissionHold, WorldRetirementHold};

pub use world::WorldMotionEvent;
pub use world::{OwnedProjectile, WorldTeleport};
pub use world::{VitalReservationDomain, VitalReservationToken};
