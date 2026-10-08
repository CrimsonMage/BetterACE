use crate::ConfigError;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};

/// Local host management is independent of game sockets and account storage.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct HostConfig {
    pub bind_address: SocketAddr,
    pub state_directory: PathBuf,
    pub log_directory: PathBuf,
    pub session_idle_seconds: u64,
    pub session_lifetime_seconds: u64,
    pub max_sessions: usize,
    pub max_log_streams: usize,
    pub startup_timeout_seconds: u64,
    pub drain_timeout_seconds: u64,
}
impl Default for HostConfig {
    fn default() -> Self {
        Self {
            bind_address: SocketAddr::from(([127, 0, 0, 1], 8080)),
            state_directory: "state/host".into(),
            log_directory: "state/logs".into(),
            session_idle_seconds: 1800,
            session_lifetime_seconds: 28800,
            max_sessions: 8,
            max_log_streams: 8,
            startup_timeout_seconds: 60,
            drain_timeout_seconds: 30,
        }
    }
}
impl HostConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.bind_address.ip().is_loopback() || self.bind_address.port() == 0 {
            return Err(ConfigError::Invalid(
                "host address must be loopback with a nonzero port",
            ));
        }
        if self.state_directory.as_os_str().is_empty()
            || self.log_directory.as_os_str().is_empty()
            || !(1..=32).contains(&self.max_sessions)
            || !(1..=16).contains(&self.max_log_streams)
            || !(60..=3600).contains(&self.session_idle_seconds)
            || !(self.session_idle_seconds..=86400).contains(&self.session_lifetime_seconds)
            || !(1..=300).contains(&self.startup_timeout_seconds)
            || !(1..=300).contains(&self.drain_timeout_seconds)
        {
            return Err(ConfigError::Invalid(
                "host limits or directories are invalid",
            ));
        }
        Ok(())
    }
    pub fn operator_file(&self) -> PathBuf {
        self.state_directory.join("operator.toml")
    }
}
