use serde::{Deserialize, Serialize};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_SERVER_TOML: &str = include_str!("default_server.toml");
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

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
    /// Optional PostgreSQL URL; the named environment variable takes precedence.
    #[serde(default)]
    pub database_url: Option<String>,
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
            database_url: None,
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
    /// Resolve the PostgreSQL connection without requiring credentials in TOML.
    /// A nonempty environment value overrides the optional configured URL.
    pub fn resolve_database_url(&self) -> Result<String, ConfigError> {
        match std::env::var(&self.database_url_env) {
            Ok(url) if !url.trim().is_empty() => return Ok(url),
            Ok(_) | Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(ConfigError::Invalid(
                    "database URL environment variable must be UTF-8",
                ));
            }
        }
        self.database_url
            .clone()
            .ok_or_else(|| ConfigError::MissingDatabaseUrl(self.database_url_env.clone()))
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let source = std::fs::read_to_string(path)?;
        Self::parse(&source)
    }
    /// Load an existing configuration unchanged, or publish a complete first-run template.
    ///
    /// A temporary file is created next to `path` and linked into place only after
    /// it has been written and synced. Linking fails if another process won the
    /// create race, in which case its file is loaded without being overwritten.
    pub fn load_or_create_default(path: &Path) -> Result<(Self, bool), ConfigError> {
        match Self::load(path) {
            Ok(config) => return Ok((config, false)),
            Err(ConfigError::Io(error)) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }

        let config = Self::parse(DEFAULT_SERVER_TOML)?;
        let directory = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let (temporary_path, mut temporary) = loop {
            let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
            let temporary_path = directory.join(format!(
                ".bace-server-config-{}-{sequence}.tmp",
                std::process::id()
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
            {
                Ok(file) => break (temporary_path, file),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        };

        let write_result = temporary
            .write_all(DEFAULT_SERVER_TOML.as_bytes())
            .and_then(|()| temporary.sync_all());
        drop(temporary);
        let publication =
            write_result.and_then(|()| match std::fs::hard_link(&temporary_path, path) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(false),
                Err(error) => Err(error),
            });
        let cleanup = std::fs::remove_file(&temporary_path);
        let created = publication?;
        cleanup?;
        if created {
            Ok((config, true))
        } else {
            Self::load(path).map(|config| (config, false))
        }
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
        if self
            .database_url
            .as_ref()
            .is_some_and(|url| url.trim().is_empty())
        {
            return Err(ConfigError::Invalid("database_url must not be empty"));
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
    #[error("{0} is not set and database_url is not configured")]
    MissingDatabaseUrl(String),
}
