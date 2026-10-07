//! Tick phases and gameplay-system orchestration.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod characters;
mod kernel;
mod scenario;

pub use characters::CharacterRegistrationError;
pub use kernel::{Command, Kernel, SimulationError};
pub use scenario::synthetic_scenario;
