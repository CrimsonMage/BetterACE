//! Authoritative integration, collision and accepted physical state.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod body;
mod projectile;
pub use projectile::{ProjectileBody, ProjectileHit, ProjectileStep};
mod collision;

pub use body::{AcceptedState, Body, GeometrySpawn, PhysicsError, STEP_SECONDS};
pub use collision::SyntheticScene;
mod gdle_bsp;
mod gdle_polygon;
mod geometry_scene;
mod sweep;
pub use gdle_bsp::{
    BspCollisionNode, BspContact, BspLimits, BspPathContact, BspQueryBudget, BspQueryError,
    CollisionSphere, GdleBspCell,
};
pub use gdle_polygon::{CollisionPlane, GdlePolygon};
pub use geometry_scene::{
    CellPortalGeometry, CollisionCylinder, CollisionFace, CollisionShape, DynamicSphere,
    GeometryCell, GeometryError, GeometryMove, GeometryRegion, GeometryStep, GeometryTrace,
    StaticBsp,
};
pub use sweep::{SweepContact, sweep_spheres};

pub use gdle_bsp::sphere_path_steps;

pub use geometry_scene::players_collide;
