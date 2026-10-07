use bace_wire::{ConnectRequest, flags};
use std::net::{IpAddr, SocketAddr};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    AuthLoginRequest,
    AuthConnectResponse,
    AuthConnected,
    WorldConnected,
    TerminationStarted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    WrongState,
    BadCookie,
    WrongAddress,
    Expired,
    ClockOverflow,
    InvalidClock,
    InvalidTimeout,
}
/// The adapter supplies monotonic milliseconds and cryptographic random values.
/// Calling begin_verified_login asserts that bace-auth already verified access;
/// the wire cookie proves endpoint continuity, not account authorization.
pub struct SessionLifecycle {
    state: SessionState,
    address: IpAddr,
    cookie: Option<u64>,
    send_endpoint: Option<SocketAddr>,
    deadline_ms: u64,
    last_ms: u64,
    timeout_ms: u64,
}
impl SessionLifecycle {
    pub fn new(address: IpAddr, now_ms: u64) -> Result<Self, SessionError> {
        Self::with_auth_timeout(address, now_ms, 15_000)
    }
    pub fn with_auth_timeout(
        address: IpAddr,
        now_ms: u64,
        timeout_ms: u64,
    ) -> Result<Self, SessionError> {
        if timeout_ms == 0 {
            return Err(SessionError::InvalidTimeout);
        }
        Ok(Self {
            state: SessionState::AuthLoginRequest,
            address,
            cookie: None,
            send_endpoint: None,
            deadline_ms: now_ms
                .checked_add(timeout_ms)
                .ok_or(SessionError::ClockOverflow)?,
            last_ms: now_ms,
            timeout_ms,
        })
    }
    pub fn state(&self) -> SessionState {
        self.state
    }
    pub fn send_endpoint(&self) -> Option<SocketAddr> {
        self.send_endpoint
    }
    pub fn begin_verified_login(
        &mut self,
        challenge: ConnectRequest,
        now_ms: u64,
        session_timeout_ms: u64,
    ) -> Result<ConnectRequest, SessionError> {
        self.check_deadline(now_ms)?;
        if session_timeout_ms == 0 {
            return Err(SessionError::InvalidTimeout);
        }
        if self.state != SessionState::AuthLoginRequest {
            return Err(SessionError::WrongState);
        }
        let deadline = now_ms
            .checked_add(session_timeout_ms)
            .ok_or(SessionError::ClockOverflow)?;
        self.cookie = Some(challenge.cookie);
        self.state = SessionState::AuthConnectResponse;
        self.deadline_ms = deadline;
        self.last_ms = now_ms;
        self.timeout_ms = session_timeout_ms;
        Ok(challenge)
    }
    /// Invoke only for a decoded connect-response received on base_port + 1.
    pub fn accept_connect_response(
        &mut self,
        cookie: u64,
        endpoint: SocketAddr,
        now_ms: u64,
    ) -> Result<(), SessionError> {
        self.check_deadline(now_ms)?;
        if self.state != SessionState::AuthConnectResponse {
            return Err(SessionError::WrongState);
        }
        if endpoint.ip() != self.address {
            return Err(SessionError::WrongAddress);
        }
        if self.cookie != Some(cookie) {
            return Err(SessionError::BadCookie);
        }
        let deadline = now_ms
            .checked_add(self.timeout_ms)
            .ok_or(SessionError::ClockOverflow)?;
        self.cookie = None;
        self.send_endpoint = Some(endpoint);
        self.state = SessionState::AuthConnected;
        self.last_ms = now_ms;
        self.deadline_ms = deadline;
        Ok(())
    }
    /// Server-side transition after assets, character ownership and world entry
    /// are committed. This MUST NOT be driven directly by a client flag.
    pub fn world_entry_committed(&mut self) -> Result<(), SessionError> {
        if self.state != SessionState::AuthConnected {
            return Err(SessionError::WrongState);
        }
        self.state = SessionState::WorldConnected;
        Ok(())
    }
    pub fn allows_flags(&self, value: u32) -> bool {
        if self.state == SessionState::TerminationStarted {
            return false;
        }
        if value & flags::LOGIN_REQUEST != 0 && self.state != SessionState::AuthLoginRequest {
            return false;
        }
        if value & flags::CONNECT_RESPONSE != 0 && self.state != SessionState::AuthConnectResponse {
            return false;
        }
        !(self.state == SessionState::AuthLoginRequest
            && value & (flags::ACK_SEQUENCE | flags::TIME_SYNC | flags::ECHO_REQUEST | flags::FLOW)
                != 0)
    }
    pub fn terminate(&mut self) {
        self.state = SessionState::TerminationStarted;
        self.cookie = None;
    }
    pub fn deadline_ms(&self) -> u64 {
        self.deadline_ms
    }

    /// Only the endpoint/checksum-validated driver may refresh liveness. Invalid
    /// datagrams and stale asynchronous authentication completions cannot do so.
    pub fn validated_activity(&mut self, now_ms: u64) -> Result<(), SessionError> {
        self.check_deadline(now_ms)?;
        if self.state == SessionState::TerminationStarted {
            return Err(SessionError::WrongState);
        }
        let deadline = now_ms
            .checked_add(self.timeout_ms)
            .ok_or(SessionError::ClockOverflow)?;
        self.deadline_ms = deadline;
        self.last_ms = now_ms;
        Ok(())
    }

    /// Called after the simulation/persistence owners acknowledge logout drain.
    pub fn logout_committed(&mut self) -> Result<(), SessionError> {
        if self.state != SessionState::WorldConnected {
            return Err(SessionError::WrongState);
        }
        self.state = SessionState::AuthConnected;
        Ok(())
    }
    fn check_deadline(&self, now_ms: u64) -> Result<(), SessionError> {
        if now_ms < self.last_ms {
            return Err(SessionError::InvalidClock);
        }
        if now_ms >= self.deadline_ms {
            Err(SessionError::Expired)
        } else {
            Ok(())
        }
    }
}
