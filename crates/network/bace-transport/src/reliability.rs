use crate::TransportError;
use bace_wire::{Datagram, PacketHeader, Reader, SERVER_DATAGRAM_LIMIT, flags};
use std::collections::BTreeMap;
#[derive(Debug, PartialEq, Eq)]
pub struct OrderingResult {
    pub ready: Vec<Vec<u8>>,
    pub duplicate: bool,
}
/// Per-peer whole-datagram ordering AFTER checksum validation. Initial ACE
/// sequence is 1. Sequence exhaustion requires reconnect instead of wraparound.
pub struct OrderedPackets {
    last: u32,
    pending: BTreeMap<u32, Vec<u8>>,
    max_packets: usize,
    max_bytes: usize,
    bytes: usize,
    window: u32,
}
impl OrderedPackets {
    pub fn new(last: u32, max_packets: usize, max_bytes: usize, window: u32) -> Self {
        Self {
            last,
            pending: BTreeMap::new(),
            max_packets,
            max_bytes,
            bytes: 0,
            window,
        }
    }
    pub fn highest_pending(&self) -> Option<u32> {
        self.pending.last_key_value().map(|(sequence, _)| *sequence)
    }
    pub fn last_received(&self) -> u32 {
        self.last
    }
    pub fn receive(&mut self, bytes: Vec<u8>) -> Result<OrderingResult, TransportError> {
        let packet = Datagram::decode(&bytes)?;
        if self.last == u32::MAX {
            return Err(TransportError::SequenceExhausted);
        }
        let sequence = packet.header.sequence;
        let packet_flags = packet.header.flags;
        if bytes.len() > self.max_bytes {
            return Err(TransportError::Capacity);
        }
        if sequence == 0 || (packet_flags == flags::ACK_SEQUENCE && sequence == self.last) {
            return Ok(OrderingResult {
                ready: vec![bytes],
                duplicate: false,
            });
        }
        if sequence <= self.last {
            return Ok(OrderingResult {
                ready: vec![],
                duplicate: true,
            });
        }
        let expected = self
            .last
            .checked_add(1)
            .ok_or(TransportError::SequenceExhausted)?;
        if sequence - expected > self.window {
            return Err(TransportError::SequenceWindow);
        }
        if sequence != expected {
            if self.pending.contains_key(&sequence) {
                return Ok(OrderingResult {
                    ready: vec![],
                    duplicate: true,
                });
            }
            if self.pending.len() >= self.max_packets
                || self.bytes.saturating_add(bytes.len()) > self.max_bytes
            {
                return Err(TransportError::Capacity);
            }
            self.bytes += bytes.len();
            self.pending.insert(sequence, bytes);
            return Ok(OrderingResult {
                ready: vec![],
                duplicate: false,
            });
        }
        let mut ready = vec![bytes];
        if packet_flags == flags::ACK_SEQUENCE {
            return Ok(OrderingResult {
                ready,
                duplicate: false,
            });
        }
        self.last = sequence;
        while let Some(next) = self.last.checked_add(1) {
            let Some(bytes) = self.pending.remove(&next) else {
                break;
            };
            self.bytes -= bytes.len();
            let header = Datagram::decode(&bytes)?.header;
            ready.push(bytes);
            if header.flags == flags::ACK_SEQUENCE {
                break;
            }
            self.last = next;
        }
        Ok(OrderingResult {
            ready,
            duplicate: false,
        })
    }
    pub fn missing(&self, before: u32) -> Vec<u32> {
        let Some(start) = self.last.checked_add(1) else {
            return vec![];
        };
        (start..before.min(start.saturating_add(self.window)))
            .filter(|sequence| !self.pending.contains_key(sequence))
            .take(115)
            .collect()
    }
}
pub struct RetransmitCache {
    entries: BTreeMap<u32, (u64, Vec<u8>)>,
    max_packets: usize,
    max_bytes: usize,
    bytes: usize,
    lifetime_ms: u64,
    last_ms: u64,
    next_expiry_ms: Option<u64>,
}
impl RetransmitCache {
    pub fn new(max_packets: usize, max_bytes: usize, lifetime_ms: u64) -> Self {
        Self {
            entries: BTreeMap::new(),
            max_packets,
            max_bytes,
            bytes: 0,
            lifetime_ms,
            last_ms: 0,
            next_expiry_ms: None,
        }
    }
    /// Reserve room for a full-sized reliable datagram before consuming the
    /// next message fragment. ACK processing or expiry will free this window.
    pub fn can_store(&self, max_datagram_bytes: usize) -> bool {
        self.entries.len() < self.max_packets
            && self.bytes.saturating_add(max_datagram_bytes) <= self.max_bytes
    }
    pub fn expire(&mut self, now_ms: u64) -> Result<(), TransportError> {
        if now_ms < self.last_ms {
            return Err(TransportError::InvalidClock);
        }
        self.last_ms = now_ms;
        if self.next_expiry_ms.is_none_or(|next| now_ms < next) {
            return Ok(());
        }
        self.next_expiry_ms = None;
        self.entries.retain(|_, (created, bytes)| {
            if now_ms - *created > self.lifetime_ms {
                self.bytes -= bytes.len();
                false
            } else {
                let deadline = created.saturating_add(self.lifetime_ms).saturating_add(1);
                self.next_expiry_ms = Some(
                    self.next_expiry_ms
                        .map_or(deadline, |old| old.min(deadline)),
                );
                true
            }
        });
        Ok(())
    }
    pub fn insert(&mut self, bytes: Vec<u8>, now_ms: u64) -> Result<(), TransportError> {
        if bytes.len() > SERVER_DATAGRAM_LIMIT {
            return Err(TransportError::Capacity);
        }
        let packet = Datagram::decode(&bytes)?;
        if packet.trailing_bytes != 0 {
            return Err(TransportError::InvalidFragment);
        }
        let sequence = packet.header.sequence;
        self.expire(now_ms)?;
        if self.entries.contains_key(&sequence) {
            return Err(TransportError::SequenceWindow);
        }
        if self.entries.len() >= self.max_packets
            || self.bytes.saturating_add(bytes.len()) > self.max_bytes
        {
            return Err(TransportError::Capacity);
        }
        self.bytes += bytes.len();
        self.entries.insert(sequence, (now_ms, bytes));
        let deadline = now_ms.saturating_add(self.lifetime_ms).saturating_add(1);
        self.next_expiry_ms = Some(
            self.next_expiry_ms
                .map_or(deadline, |old| old.min(deadline)),
        );
        Ok(())
    }
    /// ACE removes cached packets strictly below the ACK sequence.
    pub fn acknowledge(&mut self, sequence: u32) {
        self.entries.retain(|key, (_, bytes)| {
            if *key < sequence {
                self.bytes -= bytes.len();
                false
            } else {
                true
            }
        });
    }
    /// Reuses the original ISAAC contribution; only flags/header hash change.
    pub fn retransmit(&self, sequence: u32) -> Result<Option<Vec<u8>>, TransportError> {
        let Some((_, bytes)) = self.entries.get(&sequence) else {
            return Ok(None);
        };
        let mut header = PacketHeader::decode(&mut Reader::new(bytes))?;
        let payload_hash = header.checksum.wrapping_sub(header.hash());
        header.flags |= flags::RETRANSMISSION;
        header.checksum = header.hash().wrapping_add(payload_hash);
        let mut result = bytes.clone();
        result[..20].copy_from_slice(&header.encode());
        Ok(Some(result))
    }
}
