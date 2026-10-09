use super::PortalClock;
use super::types::*;
use bace_auth::LoginAttempts;
use bace_session::{
    AccountAdmission, AccountSessions, SessionKey, SessionRegistry, SessionState,
    validate_password_login,
};
use bace_transport::{EndpointKind, Peer, PeerControl, PeerSeeds, UdpEndpoints};
use bace_wire::{
    ConnectRequest, Datagram, LoginRequest, PacketHeader, decode_connect_response,
    encode_server_packet, flags, hash32,
};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Outbound {
    kind: EndpointKind,
    address: SocketAddr,
    bytes: Vec<u8>,
}
struct Connection {
    peer: Option<Peer>,
    outbound: VecDeque<Outbound>,
    termination: Option<NetworkStopReason>,
    closing_deadline: Option<u64>,
    notified: bool,
    challenge: Option<Outbound>,
    pending_login_rejection: Option<Vec<u8>>,
    login_fingerprint: [u8; 32],
}

pub(super) struct Engine {
    config: NetworkThreadConfig,
    sockets: UdpEndpoints,
    sessions: SessionRegistry,
    attempts: LoginAttempts,
    accounts: AccountSessions,
    rejected_commands: VecDeque<SessionKey>,
    batch_results: VecDeque<NetworkEvent>,
    connections: BTreeMap<SessionKey, Connection>,
    commands: Receiver<NetworkCommand>,
    events: SyncSender<NetworkEvent>,
    stop: Arc<AtomicBool>,
    start: Instant,
    clock: PortalClock,
    portal_time: f64,
    buffer: [u8; 1025],
    cursor: Option<SessionKey>,
    poll_keys: Vec<SessionKey>,
}
impl Engine {
    pub fn new(
        config: NetworkThreadConfig,
        commands: Receiver<NetworkCommand>,
        events: SyncSender<NetworkEvent>,
        stop: Arc<AtomicBool>,
    ) -> Result<Self, NetworkThreadError> {
        let sockets = UdpEndpoints::bind(config.bind_address)?;
        let sessions = SessionRegistry::with_auth_timeout(
            config.max_sessions,
            config.network.max_sessions_per_ip,
            config.network.authentication_timeout_ms,
        )
        .map_err(|e| NetworkThreadError::Startup(format!("{e:?}")))?;
        let accounts = AccountSessions::new(config.max_sessions)
            .map_err(|e| NetworkThreadError::Startup(format!("{e:?}")))?;
        let attempts = LoginAttempts::new(
            config.network.max_login_addresses,
            config.network.login_attempts_per_minute,
            60_000,
        )
        .map_err(|e| NetworkThreadError::Startup(format!("{e:?}")))?;
        let poll_keys = Vec::with_capacity(config.batch_size);
        let clock =
            PortalClock::new(config.portal_time_origin, 0).map_err(NetworkThreadError::Clock)?;
        let portal_time = config.portal_time_origin;
        Ok(Self {
            config,
            sockets,
            sessions,
            attempts,
            accounts,
            rejected_commands: VecDeque::new(),
            batch_results: VecDeque::new(),
            connections: BTreeMap::new(),
            commands,
            events,
            stop,
            start: Instant::now(),
            clock,
            portal_time,
            buffer: [0; 1025],
            cursor: None,
            poll_keys,
        })
    }
    pub fn addresses(&self) -> (SocketAddr, SocketAddr) {
        (
            self.sockets.local_address(EndpointKind::Client),
            self.sockets.local_address(EndpointKind::Server),
        )
    }
    pub fn run(&mut self) -> Result<(), NetworkThreadError> {
        while !self.stop.load(Ordering::Acquire) {
            let now_ms = u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX);
            self.portal_time = self.clock.at(now_ms).map_err(NetworkThreadError::Clock)?;
            for _ in 0..self.config.batch_size {
                let Some(result) = self.batch_results.pop_front() else {
                    break;
                };
                match self.events.try_send(result) {
                    Ok(()) => {}
                    Err(TrySendError::Full(result)) => {
                        self.batch_results.push_front(result);
                        break;
                    }
                    Err(TrySendError::Disconnected(_)) => return Err(NetworkThreadError::Closed),
                }
            }
            for _ in 0..self.config.batch_size {
                let Some(key) = self.rejected_commands.front().copied() else {
                    break;
                };
                match self.events.try_send(NetworkEvent::CommandRejected { key }) {
                    Ok(()) => {
                        self.rejected_commands.pop_front();
                    }
                    Err(TrySendError::Full(_)) => break,
                    Err(TrySendError::Disconnected(_)) => return Err(NetworkThreadError::Closed),
                }
            }
            for _ in 0..self.config.batch_size {
                if self.rejected_commands.len() == self.config.command_capacity
                    || self.batch_results.len() == self.config.command_capacity
                {
                    break;
                }
                match self.commands.try_recv() {
                    Ok(command) => self.command(command, now_ms),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(()),
                }
            }
            for kind in [EndpointKind::Client, EndpointKind::Server] {
                for _ in 0..self.config.batch_size {
                    match self.sockets.receive(kind, &mut self.buffer) {
                        Ok(Some((address, size))) => self.receive(kind, address, size, now_ms),
                        Ok(None) => break,
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::InvalidData
                                    | std::io::ErrorKind::ConnectionReset
                                    | std::io::ErrorKind::ConnectionRefused
                            ) =>
                        {
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            let expired = self
                .sessions
                .expired(now_ms)
                .map_err(|_| NetworkThreadError::Closed)?;
            for key in expired {
                self.terminate(key, NetworkStopReason::TimedOut);
            }
            // Rotate a fixed-size batch instead of favoring the lowest session ID.
            let mut keys = std::mem::take(&mut self.poll_keys);
            keys.clear();
            if let Some(cursor) = self.cursor {
                keys.extend(
                    self.connections
                        .range((
                            std::ops::Bound::Excluded(cursor),
                            std::ops::Bound::Unbounded,
                        ))
                        .map(|(key, _)| *key)
                        .take(self.config.batch_size),
                );
            }
            if keys.len() < self.config.batch_size {
                let remaining = self.config.batch_size - keys.len();
                keys.extend(
                    self.connections
                        .keys()
                        .copied()
                        .take_while(|key| self.cursor.is_none_or(|cursor| *key <= cursor))
                        .take(remaining),
                );
            }
            for key in &keys {
                self.poll(*key, now_ms);
            }
            self.cursor = keys.last().copied();
            self.poll_keys = keys;
            // Bounded idle polling, no per-peer tasks or locks. Production load
            // qualification must measure this 1ms adapter scheduling cost.
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
    fn terminate(&mut self, key: SessionKey, reason: NetworkStopReason) {
        if let Some(connection) = self.connections.get_mut(&key)
            && connection.termination.is_none()
        {
            let _ = self.sessions.terminate(key);
            if let Some(account) = self.accounts.account_for(key) {
                let _ = self.accounts.cancel(account, key);
            }
            connection.termination = Some(reason);
            connection.outbound.clear();
            connection.peer = None;
            connection.challenge = None;
        }
    }
    fn event(&mut self, key: SessionKey, event: NetworkEvent) {
        match self.events.try_send(event) {
            Ok(()) => (),
            Err(TrySendError::Full(_)) => self.terminate(key, NetworkStopReason::Overloaded),
            Err(TrySendError::Disconnected(_)) => self.stop.store(true, Ordering::Release),
        }
    }
    fn receive(&mut self, kind: EndpointKind, address: SocketAddr, size: usize, now_ms: u64) {
        let bytes = &self.buffer[..size];
        let Ok(packet) = Datagram::decode(bytes) else {
            return;
        };
        if kind == EndpointKind::Server {
            // Retail's periodic unsequenced companion-port CICMD is ignored by
            // pinned ACE; see user divergence register row 48.
            if packet.header.id == 0 && packet.header.flags == flags::CICMD_COMMAND {
                return;
            }
            if packet.header.flags != flags::CONNECT_RESPONSE
                || packet.header.checksum != packet.header.hash().wrapping_add(hash32(packet.body))
            {
                return;
            }
            let Ok(cookie) = decode_connect_response(packet.body) else {
                return;
            };
            if let Ok(key) = self.sessions.connect_response(cookie, address, now_ms) {
                if let Some(connection) = self.connections.get_mut(&key) {
                    connection.challenge = None;
                    if let Some(bytes) = connection.pending_login_rejection.take() {
                        if connection
                            .peer
                            .as_mut()
                            .is_none_or(|peer| peer.enqueue(9, bytes).is_err())
                        {
                            self.terminate(key, NetworkStopReason::Overloaded);
                            return;
                        }
                        connection.closing_deadline = Some(now_ms.saturating_add(5_000));
                        return;
                    }
                }
                if let Some(peer) = self.connections.get_mut(&key).and_then(|c| c.peer.as_mut()) {
                    peer.enable_time_sync();
                }
                self.event(key, NetworkEvent::Connected { key });
            }
            return;
        }
        if packet.header.flags == flags::LOGIN_REQUEST {
            let Ok(request) = LoginRequest::decode_datagram(bytes) else {
                return;
            };
            if self.attempts.admit(address.ip(), now_ms).is_err() {
                return;
            }
            if validate_password_login(&request).is_err() {
                return;
            }
            let fingerprint: [u8; 32] = {
                // Retry timestamps/header time may change. Hash the validated
                // credential identity fields with unambiguous lengths only.
                let login = validate_password_login(&request).expect("validated above");
                let mut digest = Sha256::new();
                for value in [
                    request.client_version.as_bytes(),
                    login.account.as_bytes(),
                    login.password.as_bytes(),
                ] {
                    digest.update((value.len() as u64).to_le_bytes());
                    digest.update(value);
                }
                digest.finalize().into()
            };
            // Bounded recovery for a lost challenge. Only an identical valid
            // login from the original endpoint can retransmit it. This differs
            // intentionally from ACE removing the old handshake on a retry.
            if let Some((key, connection)) = self.connections.iter_mut().find(|(key, _)| {
                self.sessions
                    .get(**key)
                    .is_ok_and(|s| s.endpoint() == address)
            }) {
                if connection.login_fingerprint == fingerprint
                    && self
                        .sessions
                        .get(*key)
                        .is_ok_and(|s| s.lifecycle().state() == SessionState::AuthConnectResponse)
                    && connection.outbound.len() < self.config.max_pending_datagrams_per_peer
                    && let Some(challenge) = &connection.challenge
                {
                    connection.outbound.push_back(challenge.clone());
                }
                return;
            }
            let Ok(key) = self.sessions.admit(address, now_ms) else {
                return;
            };
            self.connections.insert(
                key,
                Connection {
                    peer: None,
                    outbound: VecDeque::new(),
                    termination: None,
                    closing_deadline: None,
                    notified: false,
                    challenge: None,
                    pending_login_rejection: None,
                    login_fingerprint: fingerprint,
                },
            );
            self.event(key, NetworkEvent::Login { key, request });
            return;
        }
        let Some(key) = self.sessions.route(packet.header.id, address) else {
            return;
        };
        let Ok(session) = self.sessions.get(key) else {
            return;
        };
        if !matches!(
            session.lifecycle().state(),
            SessionState::AuthConnected | SessionState::WorldConnected
        ) || !session.lifecycle().allows_flags(packet.header.flags)
        {
            return;
        }
        let Some(peer) = self.connections.get_mut(&key).and_then(|c| c.peer.as_mut()) else {
            return;
        };
        match peer.receive(bytes, now_ms) {
            Ok(input) => {
                if input.controls.contains(&PeerControl::Disconnect) {
                    self.terminate(key, NetworkStopReason::PeerDisconnected);
                    return;
                }
                if self.sessions.validated_activity(key, now_ms).is_err() {
                    self.terminate(key, NetworkStopReason::TimedOut);
                    return;
                }
                if self
                    .connections
                    .get(&key)
                    .is_some_and(|c| c.closing_deadline.is_some())
                {
                    return;
                }
                for message in input.messages {
                    self.event(key, NetworkEvent::Message { key, message });
                    if self
                        .connections
                        .get(&key)
                        .is_none_or(|c| c.termination.is_some())
                    {
                        break;
                    }
                }
            }
            Err(_) if peer.is_closed() => self.terminate(key, NetworkStopReason::TransportError),
            Err(_) => (), // malformed/checksum failures never refresh liveness
        }
    }
    fn reject_command(&mut self, key: SessionKey) {
        // Command processing reserves capacity before consuming another input.
        // Backpressure affects admission, never unrelated established peers.
        self.rejected_commands.push_back(key);
    }
    fn command(&mut self, command: NetworkCommand, now_ms: u64) {
        match command {
            NetworkCommand::RejectBanned { key, bytes } => {
                if !self
                    .sessions
                    .get(key)
                    .is_ok_and(|s| s.lifecycle().state() == SessionState::AuthLoginRequest)
                    || bytes.len() > 4096
                {
                    self.reject_command(key);
                    return;
                }
                self.authenticate(key, now_ms);
                if let Some(connection) = self.connections.get_mut(&key)
                    && self
                        .sessions
                        .get(key)
                        .is_ok_and(|s| s.lifecycle().state() == SessionState::AuthConnectResponse)
                {
                    connection.pending_login_rejection = Some(bytes);
                } else {
                    self.terminate(key, NetworkStopReason::TransportError);
                }
            }
            NetworkCommand::Authenticated { key, account_id } => {
                if !self
                    .sessions
                    .get(key)
                    .is_ok_and(|s| s.lifecycle().state() == SessionState::AuthLoginRequest)
                {
                    self.reject_command(key);
                    return;
                }
                match self.accounts.request(account_id, key) {
                    Ok(AccountAdmission::Admitted) => self.authenticate(key, now_ms),
                    Ok(AccountAdmission::WaitingForDrain { owner }) => {
                        self.terminate(owner, NetworkStopReason::Replaced)
                    }
                    Err(_) => self.terminate(key, NetworkStopReason::AccountInUse),
                }
            }
            NetworkCommand::Send { key, queue, bytes } => {
                if !self.sessions.get(key).is_ok_and(|s| {
                    matches!(
                        s.lifecycle().state(),
                        SessionState::AuthConnected | SessionState::WorldConnected
                    )
                }) {
                    self.reject_command(key);
                } else if let Some(peer) = self
                    .connections
                    .get_mut(&key)
                    .filter(|c| c.closing_deadline.is_none())
                    .and_then(|c| c.peer.as_mut())
                {
                    if peer.enqueue(queue, bytes).is_err() {
                        self.terminate(key, NetworkStopReason::Overloaded);
                    }
                } else {
                    self.reject_command(key);
                }
            }
            NetworkCommand::SendBatch {
                key,
                queue,
                messages,
            } => {
                if !self.sessions.get(key).is_ok_and(|s| {
                    matches!(
                        s.lifecycle().state(),
                        SessionState::AuthConnected | SessionState::WorldConnected
                    )
                }) {
                    self.reject_command(key);
                } else if let Some(peer) = self
                    .connections
                    .get_mut(&key)
                    .filter(|c| c.closing_deadline.is_none())
                    .and_then(|c| c.peer.as_mut())
                {
                    for message in messages {
                        if peer.enqueue(queue, message).is_err() {
                            self.terminate(key, NetworkStopReason::Overloaded);
                            break;
                        }
                    }
                } else {
                    self.reject_command(key);
                }
            }
            NetworkCommand::SendOrderedBatch { key, messages } => {
                if !self.sessions.get(key).is_ok_and(|s| {
                    matches!(
                        s.lifecycle().state(),
                        SessionState::AuthConnected | SessionState::WorldConnected
                    )
                }) {
                    self.reject_command(key);
                } else if let Some(peer) = self
                    .connections
                    .get_mut(&key)
                    .filter(|c| c.closing_deadline.is_none())
                    .and_then(|c| c.peer.as_mut())
                {
                    for (queue, message) in messages {
                        if peer.enqueue(queue, message).is_err() {
                            self.terminate(key, NetworkStopReason::Overloaded);
                            break;
                        }
                    }
                } else {
                    self.reject_command(key);
                }
            }
            NetworkCommand::SendReliableBatch {
                key,
                correlation,
                messages,
            } => {
                let eligible = correlation != 0
                    && !messages.is_empty()
                    && self.sessions.get(key).is_ok_and(|s| {
                        matches!(
                            s.lifecycle().state(),
                            SessionState::AuthConnected | SessionState::WorldConnected
                        )
                    });
                let mut accepted = false;
                let mut overloaded = false;
                if eligible
                    && let Some(peer) = self
                        .connections
                        .get_mut(&key)
                        .filter(|c| c.closing_deadline.is_none())
                        .and_then(|c| c.peer.as_mut())
                {
                    match peer.can_enqueue_batch(&messages) {
                        Ok(true) => {
                            accepted = true;
                            for (queue, bytes) in messages {
                                if peer.enqueue(queue, bytes).is_err() {
                                    accepted = false;
                                    overloaded = true;
                                    break;
                                }
                            }
                        }
                        Ok(false) => {} // Bounded queue pressure; caller retains exact output.
                        Err(_) => overloaded = true,
                    }
                }
                if overloaded {
                    self.terminate(key, NetworkStopReason::Overloaded);
                }
                self.batch_results
                    .push_back(NetworkEvent::ReliableBatchAdmission {
                        key,
                        correlation,
                        accepted,
                    });
            }
            NetworkCommand::EnterWorldCommitted { key } => {
                if self
                    .accounts
                    .account_for(key)
                    .is_some_and(|account| self.accounts.can_enter_world(account, key))
                {
                    if self.sessions.world_entry_committed(key).is_err() {
                        self.reject_command(key);
                    }
                } else {
                    self.reject_command(key);
                }
            }
            NetworkCommand::LogoutCommitted { key } => {
                let _ = self.sessions.logout_committed(key);
            }
            NetworkCommand::TerminateAfterFlush { key, queue, bytes } => {
                if !self.sessions.get(key).is_ok_and(|s| {
                    matches!(
                        s.lifecycle().state(),
                        SessionState::AuthConnected | SessionState::WorldConnected
                    )
                }) {
                    self.reject_command(key);
                    return;
                }
                let Some(connection) = self
                    .connections
                    .get_mut(&key)
                    .filter(|c| c.closing_deadline.is_none())
                else {
                    self.reject_command(key);
                    return;
                };
                let Some(peer) = connection.peer.as_mut() else {
                    self.reject_command(key);
                    return;
                };
                if peer.enqueue(queue, bytes).is_err() {
                    self.terminate(key, NetworkStopReason::Overloaded);
                } else {
                    connection.closing_deadline = Some(now_ms.saturating_add(5_000));
                }
            }
            NetworkCommand::Terminate { key } => self.terminate(key, NetworkStopReason::Requested),
            NetworkCommand::DrainCompleted { key } => {
                if self.sessions.drain_completed(key).is_ok() {
                    self.connections.remove(&key);
                    if let Some(account) = self.accounts.account_for(key)
                        && let Ok(Some(next)) = self.accounts.drained(account, key)
                    {
                        self.authenticate(next, now_ms);
                    }
                } else {
                    self.reject_command(key);
                }
            }
        }
    }
    fn authenticate(&mut self, key: SessionKey, now_ms: u64) {
        let Ok(session) = self.sessions.get(key) else {
            return;
        };
        if session.lifecycle().state() != SessionState::AuthLoginRequest {
            return;
        }
        let address = session.endpoint();
        let mut entropy = [0u8; 16];
        if OsRng.try_fill_bytes(&mut entropy).is_err() {
            self.terminate(key, NetworkStopReason::TransportError);
            return;
        }
        let cookie = u64::from_le_bytes(entropy[..8].try_into().expect("fixed slice"));
        let server_seed = u32::from_le_bytes(entropy[8..12].try_into().expect("fixed slice"));
        let client_seed = u32::from_le_bytes(entropy[12..].try_into().expect("fixed slice"));
        let challenge = ConnectRequest {
            server_time: self.portal_time,
            cookie,
            client_id: key.id.into(),
            server_seed,
            client_seed,
        };
        let header = PacketHeader {
            sequence: 0,
            flags: flags::CONNECT_REQUEST,
            id: self.config.peer.server_id,
            iteration: 1,
            time: PortalClock::header_time(self.portal_time),
            ..PacketHeader::default()
        };
        let Ok(bytes) = encode_server_packet(header, &challenge.encode(), &[], 0) else {
            return;
        };
        let mut peer_config = self.config.peer;
        peer_config.client_id = key.id;
        let Ok(peer) = Peer::new(
            peer_config,
            PeerSeeds {
                client: client_seed,
                server: server_seed,
            },
            now_ms,
        ) else {
            self.terminate(key, NetworkStopReason::TransportError);
            return;
        };
        if self
            .sessions
            .verified_login(
                key,
                challenge,
                now_ms,
                self.config.network.session_timeout_ms,
            )
            .is_err()
        {
            return;
        }
        if let Some(connection) = self.connections.get_mut(&key) {
            connection.peer = Some(peer);
            let packet = Outbound {
                kind: EndpointKind::Client,
                address,
                bytes,
            };
            connection.challenge = Some(packet.clone());
            connection.outbound.push_back(packet);
        }
    }
    fn poll(&mut self, key: SessionKey, now_ms: u64) {
        let Some(connection) = self.connections.get_mut(&key) else {
            return;
        };
        if let Some(reason) = connection.termination {
            if !connection.notified {
                match self
                    .events
                    .try_send(NetworkEvent::Terminated { key, reason })
                {
                    Ok(()) => connection.notified = true,
                    Err(TrySendError::Full(_)) => (),
                    Err(TrySendError::Disconnected(_)) => self.stop.store(true, Ordering::Release),
                }
            }
            return;
        }
        if connection
            .closing_deadline
            .is_some_and(|deadline| now_ms >= deadline)
        {
            self.terminate(key, NetworkStopReason::TimedOut);
            return;
        }
        // Flush retained datagrams before polling the peer for additional work.
        while let Some(packet) = connection.outbound.front() {
            match self
                .sockets
                .send_to(packet.kind, packet.address, &packet.bytes)
            {
                Ok(()) => {
                    connection.outbound.pop_front();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return,
                Err(_) => {
                    self.terminate(key, NetworkStopReason::SocketError);
                    return;
                }
            }
        }
        if connection.closing_deadline.is_some()
            && connection
                .peer
                .as_ref()
                .is_some_and(|p| p.queued_message_bytes() == 0)
        {
            self.terminate(key, NetworkStopReason::Requested);
            return;
        }
        let Some(peer) = connection.peer.as_mut() else {
            return;
        };
        let Ok(session) = self.sessions.get(key) else {
            return;
        };
        if !matches!(
            session.lifecycle().state(),
            SessionState::AuthConnected | SessionState::WorldConnected
        ) {
            return;
        }
        let (kind, address) = session
            .lifecycle()
            .send_endpoint()
            .map_or((EndpointKind::Client, session.endpoint()), |address| {
                (EndpointKind::Server, address)
            });
        match peer.poll(now_ms, self.portal_time) {
            Ok(datagrams) => {
                if datagrams.len() > self.config.max_pending_datagrams_per_peer {
                    self.terminate(key, NetworkStopReason::Overloaded);
                    return;
                }
                connection
                    .outbound
                    .extend(datagrams.into_iter().map(|bytes| Outbound {
                        kind,
                        address,
                        bytes,
                    }));
            }
            Err(_) => self.terminate(key, NetworkStopReason::TransportError),
        }
    }
}
