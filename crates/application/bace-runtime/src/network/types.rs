use bace_config::NetworkConfig;
use bace_session::SessionKey;
use bace_transport::{PeerConfig, ReceivedMessage};
use bace_wire::LoginRequest;
use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub struct NetworkThreadConfig {
    pub bind_address: SocketAddr,
    /// Admitted world portal-year seconds at worker startup. Zero is a synthetic
    /// harness default; real composition must provide its authoritative origin.
    pub portal_time_origin: f64,
    pub max_sessions: usize,
    pub command_capacity: usize,
    pub event_capacity: usize,
    pub network: NetworkConfig,
    pub peer: PeerConfig,
    /// Bounded work per endpoint/command turn; peer polling also rotates fairly.
    pub batch_size: usize,
    pub max_pending_datagrams_per_peer: usize,
}
impl Default for NetworkThreadConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:9000".parse().expect("literal address"),
            portal_time_origin: 0.0,
            max_sessions: 512,
            command_capacity: 4096,
            event_capacity: 4096,
            network: NetworkConfig::default(),
            peer: PeerConfig {
                server_id: 0xB,
                ..PeerConfig::default()
            },
            batch_size: 64,
            max_pending_datagrams_per_peer: 128,
        }
    }
}
impl NetworkThreadConfig {
    pub(crate) fn validate(&self) -> Result<(), NetworkThreadError> {
        super::PortalClock::new(self.portal_time_origin, 0)
            .map_err(|error| NetworkThreadError::Startup(error.into()))?;
        self.network
            .validate()
            .map_err(|e| NetworkThreadError::Startup(e.to_string()))?;
        if !(1..=4096).contains(&self.max_sessions)
            || !(1..=65536).contains(&self.command_capacity)
            || !(1..=65536).contains(&self.event_capacity)
            || !(1..=256).contains(&self.batch_size)
            || !(1..=4096).contains(&self.max_pending_datagrams_per_peer)
        {
            return Err(NetworkThreadError::Startup(
                "network worker capacity outside supported bounds".into(),
            ));
        }
        Ok(())
    }
}
/// Commands come only from trusted application services, never decoded directly
/// from a client opcode. Successful authentication must be generation-fenced.
#[derive(Debug)]
pub enum NetworkCommand {
    Authenticated {
        key: SessionKey,
        account_id: bace_types::AccountId,
    },
    Send {
        key: SessionKey,
        queue: u16,
        bytes: Vec<u8>,
    },
    /// All messages enter the peer before polling; a partial admission failure
    /// closes the peer instead of silently sending only part of the batch.
    SendBatch {
        key: SessionKey,
        queue: u16,
        messages: Vec<Vec<u8>>,
    },
    EnterWorldCommitted {
        key: SessionKey,
    },
    LogoutCommitted {
        key: SessionKey,
    },
    Terminate {
        key: SessionKey,
    },
    DrainCompleted {
        key: SessionKey,
    },
}
#[derive(Debug)]
pub enum NetworkEvent {
    CommandRejected {
        key: SessionKey,
    },
    Login {
        key: SessionKey,
        request: LoginRequest,
    },
    Connected {
        key: SessionKey,
    },
    Message {
        key: SessionKey,
        message: ReceivedMessage,
    },
    Terminated {
        key: SessionKey,
        reason: NetworkStopReason,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkStopReason {
    Replaced,
    AccountInUse,
    Requested,
    TimedOut,
    PeerDisconnected,
    Overloaded,
    TransportError,
    SocketError,
}
#[derive(Debug, thiserror::Error)]
pub enum NetworkThreadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("network startup failed: {0}")]
    Startup(String),
    #[error("network worker closed")]
    Closed,
    #[error("network protocol clock failed: {0}")]
    Clock(&'static str),
    #[error("network worker panicked")]
    Panic,
}
