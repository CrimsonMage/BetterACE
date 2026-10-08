use crate::ConfigError;
use serde::{Deserialize, Serialize};

/// Adapter budgets, independent of the legacy wire packet sizes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkConfig {
    pub max_sessions_per_ip: usize,
    pub authentication_workers: usize,
    pub authentication_queue: usize,
    pub authentication_timeout_ms: u64,
    pub session_timeout_ms: u64,
    pub login_attempts_per_minute: u32,
    pub max_login_addresses: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            max_sessions_per_ip: 16,
            authentication_workers: 2,
            authentication_queue: 64,
            authentication_timeout_ms: 15_000,
            session_timeout_ms: 60_000,
            login_attempts_per_minute: 30,
            max_login_addresses: 4096,
        }
    }
}

impl NetworkConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=4096).contains(&self.max_sessions_per_ip)
            || !(1..=16).contains(&self.authentication_workers)
            || !(1..=4096).contains(&self.authentication_queue)
            || !(1_000..=60_000).contains(&self.authentication_timeout_ms)
            || !(1_000..=600_000).contains(&self.session_timeout_ms)
            || !(1..=1000).contains(&self.login_attempts_per_minute)
            || !(1..=65536).contains(&self.max_login_addresses)
        {
            return Err(ConfigError::Invalid(
                "network budgets are outside supported bounds",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccountConfig {
    pub allow_auto_creation: bool,
}

impl Default for AccountConfig {
    fn default() -> Self {
        Self {
            allow_auto_creation: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatDistributionConfig {
    pub enabled: bool,
}
