//! Fellowship membership and shared rewards.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.
mod membership;
pub use membership::{Fellowship, FellowshipError};
mod lifecycle;
mod rewards;
pub use rewards::{FellowRewardMember, FellowshipSharing, distance_scalar};
