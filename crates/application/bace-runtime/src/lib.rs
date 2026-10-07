//! Service composition, workers, startup and shutdown.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

pub mod authentication;
pub mod authentication_pool;
pub mod control;
pub mod dat_distribution;
pub mod entry;
mod exercise;
pub mod network;
pub mod persistence_thread;
mod publication;
pub mod saves;
pub mod simulation;
pub mod supervisor;
pub mod supervisor_child;
mod worker;

pub use publication::{PublicationError, PublicationResult, load_catalog, publish_pending_once};
pub mod character_assets;
