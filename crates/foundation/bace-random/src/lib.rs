//! Pure keyed randomness; neither entropy nor clocks belong in simulation.
mod keyed;
pub use keyed::{Domain, RandomError, RandomRoot, RandomStream};
