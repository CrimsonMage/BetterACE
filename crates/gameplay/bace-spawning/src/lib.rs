//! Generators, spawning, despawning and decay.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod schedule;
pub use schedule::{SpawnError, SpawnSchedule, SpawnTicket};
mod generator;
mod lifecycle;
mod placement;
pub use generator::{GeneratorError, GeneratorLimits, GeneratorMachine};
pub use placement::{append_generator_links, generator_destination, generator_event_stream};
