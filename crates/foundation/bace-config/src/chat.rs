//! Optional read-only integration API; credentials are separate from host users.
use crate::ConfigError;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChatApiConfig {
    pub enabled: bool,
    pub bind_address: SocketAddr,
    pub credentials_file: PathBuf,
    pub events_per_channel: usize,
    pub bytes_per_channel: usize,
    pub max_batch: usize,
    pub max_requests: usize,
    pub publication_capacity: usize,
}
impl Default for ChatApiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_address: SocketAddr::from(([127, 0, 0, 1], 8081)),
            credentials_file: "state/chat-api/bots.toml".into(),
            events_per_channel: 4096,
            bytes_per_channel: 8 * 1024 * 1024,
            max_batch: 128,
            max_requests: 32,
            publication_capacity: 4096,
        }
    }
}
impl ChatApiConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.bind_address.ip().is_loopback() || self.bind_address.port() == 0 {
            return Err(ConfigError::Invalid(
                "chat API must bind a nonzero loopback port",
            ));
        }
        if self.credentials_file.as_os_str().is_empty()
            || !(1..=65536).contains(&self.events_per_channel)
            || !(1024..=64 * 1024 * 1024).contains(&self.bytes_per_channel)
            || !(1..=1024).contains(&self.max_batch)
            || !(1..=128).contains(&self.max_requests)
            || !(1..=65536).contains(&self.publication_capacity)
        {
            return Err(ConfigError::Invalid(
                "invalid chat API capacity or credentials path",
            ));
        }
        Ok(())
    }
}
