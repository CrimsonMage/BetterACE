use crate::{Reassembly, ReassemblyLimits, TransportError};
use bace_wire::Fragment;
use std::collections::BTreeMap;

/// Fully assembled application message with original transport routing metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedMessage {
    pub sequence: u32,
    pub id: u32,
    pub queue: u16,
    pub bytes: Vec<u8>,
}

/// ACE uses a single incoming message sequence across all queues. Packet order
/// alone cannot establish message order when tails are packed ahead of bodies.
pub struct OrderedMessages {
    assembly: Reassembly,
    pending: BTreeMap<u32, (u64, ReceivedMessage)>,
    last: u32,
    bytes: usize,
    limits: ReassemblyLimits,
    window: u32,
}
impl OrderedMessages {
    pub fn new(limits: ReassemblyLimits, window: u32) -> Self {
        Self {
            assembly: Reassembly::new(limits),
            pending: BTreeMap::new(),
            last: 0,
            bytes: 0,
            limits,
            window,
        }
    }
    pub fn expire(&mut self, now_ms: u64) -> Result<(), TransportError> {
        if self.assembly.expire(now_ms)? != 0
            || self
                .pending
                .values()
                .any(|(created, _)| now_ms.saturating_sub(*created) >= self.limits.lifetime_ms)
        {
            return Err(TransportError::ExpiredMessage);
        }
        Ok(())
    }
    pub fn insert(
        &mut self,
        fragment: Fragment,
        now_ms: u64,
    ) -> Result<Vec<ReceivedMessage>, TransportError> {
        self.expire(now_ms)?;
        let header = fragment.header;
        if header.sequence <= self.last {
            return Ok(Vec::new());
        }
        if header.sequence - self.last > self.window {
            return Err(TransportError::SequenceWindow);
        }
        if let Some((_, message)) = self.pending.get(&header.sequence) {
            fragment.validate()?;
            if message.id != header.id || message.queue != header.queue {
                return Err(TransportError::ConflictingFragment);
            }
            return Ok(Vec::new());
        }
        let Some(bytes) = self.assembly.insert(fragment, now_ms)? else {
            return Ok(Vec::new());
        };
        let message = ReceivedMessage {
            sequence: header.sequence,
            id: header.id,
            queue: header.queue,
            bytes,
        };
        if header.sequence != self.last + 1 {
            if self.pending.len() >= self.limits.max_messages
                || self.bytes.saturating_add(message.bytes.len()) > self.limits.max_total_bytes
            {
                return Err(TransportError::Capacity);
            }
            self.bytes += message.bytes.len();
            self.pending.insert(header.sequence, (now_ms, message));
            return Ok(Vec::new());
        }
        self.last = header.sequence;
        let mut ready = vec![message];
        while let Some(next) = self.last.checked_add(1) {
            let Some((_, message)) = self.pending.remove(&next) else {
                break;
            };
            self.bytes -= message.bytes.len();
            ready.push(message);
            self.last = next;
        }
        Ok(ready)
    }
}
