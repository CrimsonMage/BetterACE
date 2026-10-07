use crate::{ReassemblyLimits, ReceivedMessage};

/// Engineering budgets, independent of the 484-byte server wire limit.
#[derive(Clone, Copy, Debug)]
pub struct PeerConfig {
    pub client_id: u16,
    pub server_id: u16,
    pub max_pending_packets: usize,
    pub max_pending_packet_bytes: usize,
    pub packet_window: u32,
    pub message_window: u32,
    pub reassembly: ReassemblyLimits,
    pub max_outgoing_messages: usize,
    pub max_outgoing_bytes: usize,
    pub max_cached_packets: usize,
    pub max_cached_bytes: usize,
    pub cache_lifetime_ms: u64,
    pub max_control_packets: usize,
    pub max_datagrams_per_poll: usize,
}
impl Default for PeerConfig {
    fn default() -> Self {
        Self {
            client_id: 1,
            server_id: 1,
            max_pending_packets: 256,
            max_pending_packet_bytes: 256 * 1024,
            packet_window: 256,
            message_window: 1024,
            reassembly: ReassemblyLimits::default(),
            max_outgoing_messages: 512,
            max_outgoing_bytes: 1024 * 1024,
            max_cached_packets: 4096,
            max_cached_bytes: 2 * 1024 * 1024,
            cache_lifetime_ms: 120_000,
            max_control_packets: 256,
            max_datagrams_per_poll: 64,
        }
    }
}
/// The session adapter supplies cryptographic seeds after the handshake.
#[derive(Clone, Copy, Debug)]
pub struct PeerSeeds {
    pub client: u32,
    pub server: u32,
}

/// Non-gameplay observations for the session adapter. These never update
/// authoritative position, velocity or server time.
#[derive(Clone, Debug, PartialEq)]
pub enum PeerControl {
    Disconnect,
    ClientTime(f64),
    Flow { bytes: u32, interval: u16 },
    Command([u8; 8]),
    ServerSwitch([u8; 8]),
}
#[derive(Debug, Default, PartialEq)]
pub struct PeerInput {
    pub messages: Vec<ReceivedMessage>,
    pub controls: Vec<PeerControl>,
    pub duplicate: bool,
}
