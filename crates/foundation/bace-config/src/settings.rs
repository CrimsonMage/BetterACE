use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
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
    pub command_capacity: usize,
    pub max_sessions: usize,
    pub database_connections: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            network: crate::NetworkConfig::default(),
            accounts: crate::AccountConfig::default(),
            dat_distribution: crate::DatDistributionConfig::default(),
            host: crate::HostConfig::default(),
            bind_address: "127.0.0.1:9000".into(),
            database_url_env: "BACE_DATABASE_URL".into(),
            dat_directory: None,
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
        Ok(())
    }
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
