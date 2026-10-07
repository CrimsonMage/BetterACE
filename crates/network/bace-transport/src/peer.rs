// ACE NetworkSession.cs, pinned 47edade3bd3f6044b676d4eb877c4965c7eda62b.
// Pure per-peer state: callers own sockets, admission, deadlines and thread life.
use crate::{
    Bundler, OrderedMessages, OrderedPackets, PeerConfig, PeerControl, PeerInput, PeerSeeds,
    RetransmitCache, TransportError, decode_transport_packet,
};
use bace_wire::{
    ClientKeys, Datagram, Isaac, OptionalHeaders, PacketHeader, encode_server_packet, flags,
};
use std::collections::VecDeque;

enum Reply {
    Raw(Vec<u8>),
    Control(OptionalHeaders),
}

pub struct Peer {
    config: PeerConfig,
    client_keys: ClientKeys,
    server_keys: Isaac,
    packets: OrderedPackets,
    messages: OrderedMessages,
    outgoing: Bundler,
    cache: RetransmitCache,
    replies: VecDeque<Reply>,
    last_ms: u64,
    next_bundle_ms: u64,
    next_ack_ms: u64,
    next_sync_ms: u64,
    next_nak_ms: u64,
    last_sent: u32,
    echo: Option<f32>,
    time_sync_enabled: bool,
    closed: bool,
}
impl Peer {
    /// Constructs post-handshake state. ACE starts inbound packet ordering at
    /// 1, inbound messages at 1, outbound messages at 0, encrypted packets at 2.
    pub fn new(config: PeerConfig, seeds: PeerSeeds, now_ms: u64) -> Result<Self, TransportError> {
        if config.max_pending_packets == 0
            || config.max_pending_packet_bytes < 1024
            || config.packet_window == 0
            || config.packet_window > 256
            || config.message_window == 0
            || config.max_outgoing_messages == 0
            || config.max_outgoing_bytes < 4
            || config.max_cached_packets == 0
            || config.max_cached_bytes < 484
            || config.max_control_packets == 0
            || config.max_datagrams_per_poll == 0
            || config.cache_lifetime_ms == 0
            || config.reassembly.max_messages == 0
            || config.reassembly.max_message_bytes < 4
            || config.reassembly.max_message_bytes > usize::from(u16::MAX) * 448
            || config.reassembly.max_total_bytes < config.reassembly.max_message_bytes
            || config.reassembly.lifetime_ms == 0
        {
            return Err(TransportError::InvalidConfiguration);
        }
        Ok(Self {
            config,
            client_keys: ClientKeys::new(seeds.client),
            server_keys: Isaac::new(seeds.server),
            packets: OrderedPackets::new(
                1,
                config.max_pending_packets,
                config.max_pending_packet_bytes,
                config.packet_window,
            ),
            messages: OrderedMessages::new(config.reassembly, config.message_window),
            outgoing: Bundler::new(
                config.max_outgoing_messages,
                config.max_outgoing_bytes,
                config.reassembly.max_message_bytes,
            ),
            cache: RetransmitCache::new(
                config.max_cached_packets,
                config.max_cached_bytes,
                config.cache_lifetime_ms,
            ),
            replies: VecDeque::new(),
            last_ms: now_ms,
            next_bundle_ms: now_ms,
            next_ack_ms: now_ms.saturating_add(2000),
            next_sync_ms: now_ms,
            next_nak_ms: now_ms,
            last_sent: 0,
            echo: None,
            time_sync_enabled: false,
            closed: false,
        })
    }
    pub fn enable_time_sync(&mut self) {
        self.time_sync_enabled = true;
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    pub fn last_received_sequence(&self) -> u32 {
        self.packets.last_received()
    }
    pub fn queued_message_bytes(&self) -> usize {
        self.outgoing.buffered_bytes()
    }
    pub fn enqueue(&mut self, queue: u16, bytes: Vec<u8>) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::Closed);
        }
        let result = self.outgoing.enqueue(queue, bytes);
        self.close_on_failure(result)
    }
    fn clock(&mut self, now_ms: u64) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::Closed);
        }
        if now_ms < self.last_ms {
            return Err(TransportError::InvalidClock);
        }
        self.last_ms = now_ms;
        Ok(())
    }
    fn close_on_failure<T>(
        &mut self,
        result: Result<T, TransportError>,
    ) -> Result<T, TransportError> {
        if result.is_err() {
            self.closed = true;
        }
        result
    }
    /// Returns complete ordered messages only after checksum verification.
    /// Wrong-peer, malformed and checksum-failed datagrams do not refresh any
    /// adapter-owned session deadline. Reliable state failures close the peer.
    pub fn receive(&mut self, bytes: &[u8], now_ms: u64) -> Result<PeerInput, TransportError> {
        self.clock(now_ms)?;
        let header = Datagram::decode(bytes)?.header;
        if header.id != self.config.client_id {
            return Err(TransportError::WrongPeer);
        }
        // Verify on a candidate stream: unauthenticated traffic cannot destroy
        // this peer's finite unused-key window.
        let mut candidate_keys = self.client_keys.clone();
        let packet = decode_transport_packet(bytes, &mut candidate_keys)?;
        self.client_keys = candidate_keys;
        let result = self.receive_verified(bytes, packet, now_ms);
        self.close_on_failure(result)
    }
    fn receive_verified(
        &mut self,
        bytes: &[u8],
        packet: crate::IncomingPacket,
        now_ms: u64,
    ) -> Result<PeerInput, TransportError> {
        self.messages.expire(now_ms)?;
        self.cache.expire(now_ms)?;
        if packet.header.flags & flags::REQUEST_RETRANSMIT != 0
            && packet.header.flags & flags::ENCRYPTED_CHECKSUM == 0
        {
            let mut rejected = Vec::new();
            for sequence in packet.optional.retransmit.unwrap_or_default() {
                if let Some(bytes) = self.cache.retransmit(sequence)? {
                    self.queue_reply(Reply::Raw(bytes))?;
                } else {
                    rejected.push(sequence);
                }
            }
            for chunk in rejected.chunks(115) {
                self.queue_reply(Reply::Control(OptionalHeaders {
                    reject_retransmit: Some(chunk.to_vec()),
                    ..OptionalHeaders::default()
                }))?;
            }
            return Ok(PeerInput::default());
        }
        if packet.header.flags & (flags::DISCONNECT | flags::NET_ERROR_DISCONNECT) != 0 {
            self.closed = true;
            return Ok(PeerInput {
                controls: vec![PeerControl::Disconnect],
                ..PeerInput::default()
            });
        }
        let ordered = self.packets.receive(bytes.to_vec())?;
        let mut input = PeerInput {
            duplicate: ordered.duplicate,
            ..PeerInput::default()
        };
        for bytes in ordered.ready {
            // Packets in this queue already passed checksum verification once.
            let packet = crate::incoming::parse_transport_packet(&bytes)?.0;
            if let Some(ack) = packet.optional.ack {
                if ack > self.last_sent {
                    return Err(TransportError::SequenceWindow);
                }
                self.cache.acknowledge(ack);
            }
            if packet
                .optional
                .reject_retransmit
                .as_ref()
                .is_some_and(|ids| !ids.is_empty())
            {
                return Err(TransportError::RetransmitUnavailable);
            }
            if let Some(time) = packet.optional.echo_request {
                if !time.is_finite() {
                    return Err(TransportError::InvalidClock);
                }
                self.echo = Some(time);
            }
            if let Some(time) = packet.optional.time_sync {
                if !time.is_finite() {
                    return Err(TransportError::InvalidClock);
                }
                input.controls.push(PeerControl::ClientTime(time));
            }
            if let Some((bytes, interval)) = packet.optional.flow {
                input.controls.push(PeerControl::Flow { bytes, interval });
            }
            if let Some(command) = packet.optional.command {
                input.controls.push(PeerControl::Command(command));
            }
            if let Some(server_switch) = packet.optional.server_switch {
                input
                    .controls
                    .push(PeerControl::ServerSwitch(server_switch));
            }
            for fragment in packet.fragments {
                input
                    .messages
                    .extend(self.messages.insert(fragment, now_ms)?);
            }
        }
        Ok(input)
    }
    fn queue_reply(&mut self, reply: Reply) -> Result<(), TransportError> {
        if self.replies.len() >= self.config.max_control_packets {
            return Err(TransportError::Capacity);
        }
        self.replies.push_back(reply);
        Ok(())
    }
    /// Supplies explicit monotonic milliseconds and ACE portal-year seconds.
    /// Output must be sent or retained by a bounded adapter; dropping successful
    /// output is not a supported backpressure strategy.
    pub fn poll(&mut self, now_ms: u64, portal_time: f64) -> Result<Vec<Vec<u8>>, TransportError> {
        self.clock(now_ms)?;
        if !portal_time.is_finite() || portal_time < 0.0 {
            return Err(TransportError::InvalidClock);
        }
        let result = self.poll_inner(now_ms, portal_time);
        self.close_on_failure(result)
    }
    fn poll_inner(
        &mut self,
        now_ms: u64,
        portal_time: f64,
    ) -> Result<Vec<Vec<u8>>, TransportError> {
        self.messages.expire(now_ms)?;
        self.cache.expire(now_ms)?;
        let mut result = Vec::new();
        while result.len() < self.config.max_datagrams_per_poll {
            let Some(reply) = self.replies.pop_front() else {
                break;
            };
            result.push(match reply {
                Reply::Raw(bytes) => bytes,
                Reply::Control(headers) => self.encode(headers, &[], false, now_ms, portal_time)?,
            });
        }
        if result.len() < self.config.max_datagrams_per_poll
            && now_ms > self.next_nak_ms
            && let Some(highest) = self.packets.highest_pending()
        {
            let missing = self.packets.missing(highest);
            if !missing.is_empty() {
                result.push(self.encode(
                    OptionalHeaders {
                        retransmit: Some(missing),
                        ..OptionalHeaders::default()
                    },
                    &[],
                    false,
                    now_ms,
                    portal_time,
                )?);
                self.next_nak_ms = now_ms.saturating_add(1000);
            }
        }
        if now_ms < self.next_bundle_ms {
            return Ok(result);
        }
        if result.len() < self.config.max_datagrams_per_poll {
            let mut optional = OptionalHeaders::default();
            if now_ms > self.next_ack_ms {
                optional.ack = Some(self.packets.last_received());
                self.next_ack_ms = now_ms.saturating_add(2000);
            }
            if self.time_sync_enabled && now_ms > self.next_sync_ms {
                optional.time_sync = Some(portal_time);
                self.next_sync_ms = now_ms.saturating_add(20_000);
            }
            if let Some(time) = self.echo.take() {
                optional.echo_response = Some((time, portal_time as f32 - time));
            }
            if optional != OptionalHeaders::default() {
                let encrypted = optional.time_sync.is_some() || optional.echo_response.is_some();
                result.push(self.encode(optional, &[], encrypted, now_ms, portal_time)?);
                self.next_bundle_ms = now_ms.saturating_add(5);
                return Ok(result);
            }
        }
        if self.outgoing.start_bundle()? {
            while result.len() < self.config.max_datagrams_per_poll
                && self.outgoing.has_active_bundle()
            {
                let fragments = self.outgoing.next_fragments();
                result.push(self.encode(
                    OptionalHeaders::default(),
                    &fragments,
                    true,
                    now_ms,
                    portal_time,
                )?);
            }
            if !self.outgoing.has_active_bundle() {
                self.next_bundle_ms = now_ms.saturating_add(5);
            }
        }
        Ok(result)
    }
    fn encode(
        &mut self,
        optional: OptionalHeaders,
        fragments: &[bace_wire::Fragment],
        encrypted: bool,
        now_ms: u64,
        portal_time: f64,
    ) -> Result<Vec<u8>, TransportError> {
        let (mut bits, bytes) = optional.encode()?;
        if encrypted {
            if self.last_sent == 0 {
                self.last_sent = 1;
            }
            bits |= flags::ENCRYPTED_CHECKSUM;
        }
        if !fragments.is_empty() {
            bits |= flags::BLOB_FRAGMENTS;
        }
        let increments = bits != flags::ACK_SEQUENCE && bits & flags::REQUEST_RETRANSMIT == 0;
        if increments {
            self.last_sent = self
                .last_sent
                .checked_add(1)
                .ok_or(TransportError::SequenceExhausted)?;
        }
        let key = if encrypted {
            self.server_keys.next_key()
        } else {
            0
        };
        let bytes = encode_server_packet(
            PacketHeader {
                sequence: self.last_sent,
                flags: bits,
                id: self.config.server_id,
                time: portal_time.trunc().rem_euclid(65536.0) as u16,
                iteration: 1,
                ..PacketHeader::default()
            },
            &bytes,
            fragments,
            key,
        )?;
        if increments {
            self.cache.insert(bytes.clone(), now_ms)?;
        }
        Ok(bytes)
    }
}
