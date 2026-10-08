//! Scheduled and content-controlled world events.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.
mod events;
pub use events::{EventDefinition, EventError, EventState, Events};
