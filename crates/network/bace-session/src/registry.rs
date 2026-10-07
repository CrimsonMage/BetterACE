use crate::{SessionError, SessionLifecycle, SessionState};
use bace_wire::ConnectRequest;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};

// One allocation per admitted login, never per packet/tick. Process-wide IDs
// also fence delayed work across network-worker replacement. Durable ownership
// still uses repository epochs; session generations are not save identifiers.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Slot generations fence all worker results; legacy u16 IDs alone are unsafe
/// when a terminated endpoint's slot is reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionKey {
    pub id: u16,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    InvalidLimits,
    Full,
    AddressLimit,
    EndpointInUse,
    StaleSession,
    ChallengeMismatch,
    InvalidClock,
    GenerationExhausted,
    Lifecycle(SessionError),
    DrainRequired,
}
impl From<SessionError> for AdmissionError {
    fn from(value: SessionError) -> Self {
        Self::Lifecycle(value)
    }
}

pub struct RegisteredSession {
    key: SessionKey,
    endpoint: SocketAddr,
    lifecycle: SessionLifecycle,
    cookie: Option<u64>,
}
impl RegisteredSession {
    pub fn key(&self) -> SessionKey {
        self.key
    }
    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }
    pub fn lifecycle(&self) -> &SessionLifecycle {
        &self.lifecycle
    }
}

/// Single network-owner registry. Termination retains its slot until the
/// adapter confirms owner/drain completion; expiry never frees a world owner.
pub struct SessionRegistry {
    slots: Vec<Option<RegisteredSession>>,
    max_per_ip: usize,
    last_ms: u64,
    auth_timeout_ms: u64,
}
impl SessionRegistry {
    pub fn new(max_sessions: usize, max_per_ip: usize) -> Result<Self, AdmissionError> {
        Self::with_auth_timeout(max_sessions, max_per_ip, 15_000)
    }
    pub fn with_auth_timeout(
        max_sessions: usize,
        max_per_ip: usize,
        auth_timeout_ms: u64,
    ) -> Result<Self, AdmissionError> {
        if !(1..=4096).contains(&max_sessions)
            || !(1..=4096).contains(&max_per_ip)
            || auth_timeout_ms == 0
        {
            return Err(AdmissionError::InvalidLimits);
        }
        Ok(Self {
            slots: (0..max_sessions).map(|_| None).collect(),
            max_per_ip,
            last_ms: 0,
            auth_timeout_ms,
        })
    }
    pub fn admit(
        &mut self,
        endpoint: SocketAddr,
        now_ms: u64,
    ) -> Result<SessionKey, AdmissionError> {
        self.clock(now_ms)?;
        let active = || self.slots.iter().flatten();
        if active().any(|session| session.endpoint == endpoint) {
            return Err(AdmissionError::EndpointInUse);
        }
        if active()
            .filter(|session| session.endpoint.ip() == endpoint.ip())
            .count()
            >= self.max_per_ip
        {
            return Err(AdmissionError::AddressLimit);
        }
        let index = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(AdmissionError::Full)?;
        let lifecycle =
            SessionLifecycle::with_auth_timeout(endpoint.ip(), now_ms, self.auth_timeout_ms)?;
        let generation = NEXT_GENERATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| AdmissionError::GenerationExhausted)?;
        let key = SessionKey {
            id: index as u16,
            generation,
        };
        self.slots[index] = Some(RegisteredSession {
            key,
            endpoint,
            lifecycle,
            cookie: None,
        });
        Ok(key)
    }
    pub fn get(&self, key: SessionKey) -> Result<&RegisteredSession, AdmissionError> {
        self.slots
            .get(usize::from(key.id))
            .and_then(Option::as_ref)
            .filter(|s| s.key == key)
            .ok_or(AdmissionError::StaleSession)
    }
    fn get_mut(&mut self, key: SessionKey) -> Result<&mut RegisteredSession, AdmissionError> {
        self.slots
            .get_mut(usize::from(key.id))
            .and_then(Option::as_mut)
            .filter(|s| s.key == key)
            .ok_or(AdmissionError::StaleSession)
    }
    pub fn route(&self, id: u16, endpoint: SocketAddr) -> Option<SessionKey> {
        self.slots
            .get(usize::from(id))
            .and_then(Option::as_ref)
            .filter(|s| s.endpoint == endpoint)
            .map(|s| s.key)
    }
    /// Invoke only after bace-auth returns a verified account. No client packet
    /// can assert successful account authentication through this registry.
    pub fn verified_login(
        &mut self,
        key: SessionKey,
        challenge: ConnectRequest,
        now_ms: u64,
        timeout_ms: u64,
    ) -> Result<(), AdmissionError> {
        self.clock(now_ms)?;
        if challenge.client_id != u32::from(key.id)
            || self
                .slots
                .iter()
                .flatten()
                .any(|s| s.cookie == Some(challenge.cookie))
        {
            return Err(AdmissionError::ChallengeMismatch);
        }
        let session = self.get_mut(key)?;
        session
            .lifecycle
            .begin_verified_login(challenge, now_ms, timeout_ms)?;
        session.cookie = Some(challenge.cookie);
        Ok(())
    }
    /// Companion-port connect responses are located by cookie and original IP.
    pub fn connect_response(
        &mut self,
        cookie: u64,
        endpoint: SocketAddr,
        now_ms: u64,
    ) -> Result<SessionKey, AdmissionError> {
        self.clock(now_ms)?;
        let key = self
            .slots
            .iter()
            .flatten()
            .find(|s| s.cookie == Some(cookie) && s.endpoint.ip() == endpoint.ip())
            .map(|s| s.key)
            .ok_or(AdmissionError::ChallengeMismatch)?;
        let session = self.get_mut(key)?;
        session
            .lifecycle
            .accept_connect_response(cookie, endpoint, now_ms)?;
        session.lifecycle.validated_activity(now_ms)?;
        session.cookie = None;
        Ok(key)
    }
    pub fn validated_activity(
        &mut self,
        key: SessionKey,
        now_ms: u64,
    ) -> Result<(), AdmissionError> {
        self.clock(now_ms)?;
        self.get_mut(key)?.lifecycle.validated_activity(now_ms)?;
        Ok(())
    }
    pub fn world_entry_committed(&mut self, key: SessionKey) -> Result<(), AdmissionError> {
        self.get_mut(key)?.lifecycle.world_entry_committed()?;
        Ok(())
    }
    pub fn logout_committed(&mut self, key: SessionKey) -> Result<(), AdmissionError> {
        self.get_mut(key)?.lifecycle.logout_committed()?;
        Ok(())
    }
    pub fn terminate(&mut self, key: SessionKey) -> Result<(), AdmissionError> {
        let session = self.get_mut(key)?;
        session.lifecycle.terminate();
        session.cookie = None;
        Ok(())
    }
    /// This acknowledgment must originate from owning services, never clients.
    pub fn drain_completed(&mut self, key: SessionKey) -> Result<(), AdmissionError> {
        if self.get(key)?.lifecycle.state() != SessionState::TerminationStarted {
            return Err(AdmissionError::DrainRequired);
        }
        self.slots[usize::from(key.id)] = None;
        Ok(())
    }
    pub fn expired(&mut self, now_ms: u64) -> Result<Vec<SessionKey>, AdmissionError> {
        self.clock(now_ms)?;
        Ok(self
            .slots
            .iter()
            .flatten()
            .filter(|s| {
                s.lifecycle.state() != SessionState::TerminationStarted
                    && now_ms >= s.lifecycle.deadline_ms()
            })
            .map(|s| s.key)
            .collect())
    }
    pub fn len(&self) -> usize {
        self.slots.iter().flatten().count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn clock(&mut self, now_ms: u64) -> Result<(), AdmissionError> {
        if now_ms < self.last_ms {
            return Err(AdmissionError::InvalidClock);
        }
        self.last_ms = now_ms;
        Ok(())
    }
}
