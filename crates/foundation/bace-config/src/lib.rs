//! TOML configuration loading and validation.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod chat;
mod host;
mod network;
mod settings;
mod world;
mod world_defaults;

pub use chat::ChatApiConfig;
pub use host::HostConfig;
pub use network::{AccountConfig, DatDistributionConfig, NetworkConfig};
pub use settings::{ConfigError, ServerConfig};
pub use world::{
    PlayerDeathConfig, PreloadEntry, PreloadingConfig, RecallConfig, WorldConfig, ZoneRulesConfig,
};

mod chat_policy;
pub use chat_policy::ChatPolicyConfig;
