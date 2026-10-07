//! Bounded authoritative replication sequencing and observer knowledge.
//! Complete world/object projections remain a separate integration gate.
mod sequences;
mod visibility;
pub use sequences::{ReplicationError, SequenceKind, Sequences};
pub use visibility::{ForgetTicket, Visibility, VisibilityChange};
mod progression;
pub use progression::{ProgressionPackets, ProgressionProjectionError, ProgressionProjector};
