//! TOML configuration loading and validation.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod host;
mod network;
mod settings;

pub use host::HostConfig;
pub use network::{AccountConfig, DatDistributionConfig, NetworkConfig};
pub use settings::{ConfigError, ServerConfig};
