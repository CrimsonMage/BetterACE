use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default)]
    pub chat_policy: crate::ChatPolicyConfig,
    #[serde(default)]
    pub chat_api: crate::ChatApiConfig,
    #[serde(default)]
    pub world: crate::WorldConfig,
    #[serde(default)]
    pub network: crate::NetworkConfig,
    #[serde(default)]
    pub accounts: crate::AccountConfig,
    #[serde(default)]
    pub dat_distribution: crate::DatDistributionConfig,
    #[serde(default)]
    pub host: crate::HostConfig,
    pub bind_address: String,
    pub database_url_env: String,
    pub dat_directory: Option<PathBuf>,
    /// Immutable aggregate world packs; PostgreSQL selects the accepted manifest.
    #[serde(default)]
    pub pack_directory: Option<PathBuf>,
    /// Operator-reviewed native TOML inbox; never scanned or imported on startup.
    #[serde(default = "default_content_inbox_directory")]
    pub content_inbox_directory: PathBuf,
    /// Provisioned private RNG key; missing/replaced keys must block random gameplay.
    #[serde(default)]
    pub random_key_file: Option<PathBuf>,
    /// Accepted `.bace` namespace-52 loot table-set ID; 1 is pinned ACE.
    #[serde(default = "default_treasure_table_set_id")]
    pub treasure_table_set_id: u32,
    pub command_capacity: usize,
    pub max_sessions: usize,
    pub database_connections: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            chat_policy: crate::ChatPolicyConfig::default(),
            chat_api: crate::ChatApiConfig::default(),
            world: crate::WorldConfig::default(),
            network: crate::NetworkConfig::default(),
            accounts: crate::AccountConfig::default(),
            dat_distribution: crate::DatDistributionConfig::default(),
            host: crate::HostConfig::default(),
            bind_address: "127.0.0.1:9000".into(),
            database_url_env: "BACE_DATABASE_URL".into(),
            dat_directory: None,
            pack_directory: None,
            content_inbox_directory: default_content_inbox_directory(),
            random_key_file: None,
            treasure_table_set_id: 1,
            command_capacity: 4096,
            max_sessions: 512,
            database_connections: 4,
        }
    }
}

impl ServerConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let source = std::fs::read_to_string(path)?;
        Self::parse(&source)
    }
    pub fn parse(source: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(source)?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.chat_api.validate()?;
        self.world.validate()?;
        self.host.validate()?;
        self.network.validate()?;
        if self.dat_distribution.enabled && self.dat_directory.is_none() {
            return Err(ConfigError::Invalid(
                "DAT distribution requires dat_directory",
            ));
        }
        let address: std::net::SocketAddr = self
            .bind_address
            .parse()
            .map_err(|_| ConfigError::Invalid("bind_address must be an IP:port"))?;
        if address.port() == 0 || address.port() == u16::MAX {
            return Err(ConfigError::Invalid(
                "bind port must leave room for the ACE companion port",
            ));
        }
        if !(1..=65536).contains(&self.command_capacity)
            || !(1..=4096).contains(&self.max_sessions)
            || !(1..=32).contains(&self.database_connections)
        {
            return Err(ConfigError::Invalid(
                "configured queues/connections exceed supported bounds",
            ));
        }
        if self.database_url_env.is_empty()
            || !self
                .database_url_env
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(ConfigError::Invalid(
                "database_url_env must name an environment variable",
            ));
        }
        if self.content_inbox_directory.as_os_str().is_empty() {
            return Err(ConfigError::Invalid(
                "content_inbox_directory must not be empty",
            ));
        }
        if self.treasure_table_set_id == 0 {
            return Err(ConfigError::Invalid(
                "treasure_table_set_id must be positive",
            ));
        }
        Ok(())
    }
}

fn default_content_inbox_directory() -> PathBuf {
    "state/content-inbox".into()
}
fn default_treasure_table_set_id() -> u32 {
    1
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Invalid(&'static str),
}
