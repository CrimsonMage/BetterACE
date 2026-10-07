//! Authoritative integration, collision and accepted physical state.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod body;
mod collision;

pub use body::{AcceptedState, Body, PhysicsError, STEP_SECONDS};
pub use collision::SyntheticScene;
