//! Bounded reliable per-peer networking with explicit clocks and seeds.
mod error;
mod fragments;
mod incoming;
mod reliability;
pub use error::TransportError;
pub use fragments::{Reassembly, ReassemblyLimits, fragment_message};
pub use incoming::{IncomingPacket, decode_transport_packet};
pub use reliability::{OrderedPackets, OrderingResult, RetransmitCache};

mod bundling;
mod messages;
mod peer;
mod peer_types;
pub use bundling::Bundler;
pub use messages::{OrderedMessages, ReceivedMessage};
pub use peer::Peer;
pub use peer_types::{PeerConfig, PeerControl, PeerInput, PeerSeeds};
mod udp;
pub use udp::{EndpointKind, UdpEndpoints};
