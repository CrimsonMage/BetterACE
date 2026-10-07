//! Bounded dirty-save coordination and opaque persistence contracts.
mod contracts;
mod dirty;
pub use contracts::*;
pub use dirty::*;
mod mapped;
pub use mapped::*;
mod offline;
pub use offline::*;
