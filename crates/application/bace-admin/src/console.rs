use crate::{HostError, auth::AuthState};
use bace_auth::{PasswordHashRecord, PasswordService};
use bace_config::HostConfig;
use bace_observability::LogStore;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::{Semaphore, mpsc, oneshot, watch};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostStatus {
    pub phase: String,
    pub generation: u64,
    pub child_pid: Option<u32>,
    pub game_ready: bool,
    pub detail: String,
    pub operation_id: Option<u64>,
}
impl Default for HostStatus {
    fn default() -> Self {
        Self {
            phase: "stopped".into(),
            generation: 0,
            child_pid: None,
            game_ready: false,
            detail: "Game serving unsupported; no stock-client world is running.".into(),
            operation_id: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlAction {
    Restart,
    Retry,
}
pub struct ControlRequest {
    pub action: ControlAction,
    pub reply: oneshot::Sender<Result<u64, String>>,
}

#[derive(Clone)]
pub struct HostConsole {
    pub(crate) inner: Arc<ConsoleState>,
}
pub(crate) struct ConsoleState {
    pub config: HostConfig,
    pub authority: String,
    pub origin: String,
    pub password: PasswordHashRecord,
    pub passwords: Arc<PasswordService>,
    pub auth: Mutex<AuthState>,
    pub streams: Arc<Semaphore>,
    pub requests: Arc<Semaphore>,
    pub logs: Arc<LogStore>,
    pub status: watch::Receiver<HostStatus>,
    pub control: mpsc::Sender<ControlRequest>,
}
impl HostConsole {
    pub fn new(
        config: HostConfig,
        password: PasswordHashRecord,
        logs: Arc<LogStore>,
        status: watch::Receiver<HostStatus>,
        control: mpsc::Sender<ControlRequest>,
    ) -> Result<Self, HostError> {
        config.validate().map_err(|_| HostError::Credentials)?;
        let authority = config.bind_address.to_string();
        Ok(Self {
            inner: Arc::new(ConsoleState {
                origin: format!("http://{authority}"),
                authority,
                streams: Arc::new(Semaphore::new(config.max_log_streams)),
                requests: Arc::new(Semaphore::new(32)),
                config,
                password,
                passwords: Arc::new(PasswordService::new(2).map_err(|_| HostError::Credentials)?),
                auth: Mutex::new(AuthState::default()),
                logs,
                status,
                control,
            }),
        })
    }
}
