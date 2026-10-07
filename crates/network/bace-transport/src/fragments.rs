use crate::TransportError;
use bace_wire::{FRAGMENT_DATA_LIMIT, Fragment, FragmentHeader};
use std::collections::BTreeMap;

pub fn fragment_message(
    sequence: u32,
    queue: u16,
    bytes: &[u8],
    max_message: usize,
) -> Result<Vec<Fragment>, TransportError> {
    if bytes.len() < 4 || bytes.len() > max_message || queue >= 12 {
        return Err(TransportError::InvalidFragment);
    }
    let count = u16::try_from(bytes.len().div_ceil(FRAGMENT_DATA_LIMIT))
        .map_err(|_| TransportError::Capacity)?;
    Ok(bytes
        .chunks(FRAGMENT_DATA_LIMIT)
        .enumerate()
        .map(|(index, data)| Fragment {
            header: FragmentHeader {
                sequence,
                id: 0x80000000,
                count,
                size: (data.len() + 16) as u16,
                index: index as u16,
                queue,
            },
            data: data.to_vec(),
        })
        .collect())
}
#[derive(Clone, Copy, Debug)]
pub struct ReassemblyLimits {
    pub max_messages: usize,
    pub max_message_bytes: usize,
    pub max_total_bytes: usize,
    pub lifetime_ms: u64,
}
impl Default for ReassemblyLimits {
    fn default() -> Self {
        Self {
            max_messages: 64,
            max_message_bytes: 256 * 1024,
            max_total_bytes: 1024 * 1024,
            lifetime_ms: 30_000,
        }
    }
}
struct Partial {
    count: u16,
    id: u32,
    queue: u16,
    created_ms: u64,
    parts: BTreeMap<u16, Vec<u8>>,
    bytes: usize,
}
pub struct Reassembly {
    limits: ReassemblyLimits,
    messages: BTreeMap<u32, Partial>,
    bytes: usize,
    last_ms: u64,
}
impl Reassembly {
    pub fn new(limits: ReassemblyLimits) -> Self {
        Self {
            limits,
            messages: BTreeMap::new(),
            bytes: 0,
            last_ms: 0,
        }
    }
    pub fn buffered_bytes(&self) -> usize {
        self.bytes
    }
    pub fn pending_messages(&self) -> usize {
        self.messages.len()
    }
    pub fn expire(&mut self, now_ms: u64) -> Result<usize, TransportError> {
        if now_ms < self.last_ms {
            return Err(TransportError::InvalidClock);
        }
        self.last_ms = now_ms;
        let mut removed = 0;
        self.messages.retain(|_, message| {
            if now_ms - message.created_ms >= self.limits.lifetime_ms {
                self.bytes -= message.bytes;
                removed += 1;
                false
            } else {
                true
            }
        });
        Ok(removed)
    }
    pub fn insert(
        &mut self,
        fragment: Fragment,
        now_ms: u64,
    ) -> Result<Option<Vec<u8>>, TransportError> {
        fragment.validate()?;
        self.expire(now_ms)?;
        let header = fragment.header;
        if usize::from(header.count) > self.limits.max_message_bytes.div_ceil(FRAGMENT_DATA_LIMIT) {
            return Err(TransportError::Capacity);
        }
        if header.count == 1 {
            if self.messages.contains_key(&header.sequence) {
                return Err(TransportError::ConflictingFragment);
            }
            if fragment.data.len() < 4 || fragment.data.len() > self.limits.max_message_bytes {
                return Err(TransportError::InvalidFragment);
            }
            return Ok(Some(fragment.data));
        }
        let existing = self.messages.get(&header.sequence);
        if let Some(message) = existing {
            if (message.count, message.id, message.queue) != (header.count, header.id, header.queue)
            {
                return Err(TransportError::ConflictingFragment);
            }
            if let Some(previous) = message.parts.get(&header.index) {
                return if previous == &fragment.data {
                    Ok(None)
                } else {
                    Err(TransportError::ConflictingFragment)
                };
            }
            if message.bytes.saturating_add(fragment.data.len()) > self.limits.max_message_bytes {
                return Err(TransportError::Capacity);
            }
        } else if self.messages.len() >= self.limits.max_messages {
            return Err(TransportError::Capacity);
        }
        if self.bytes.saturating_add(fragment.data.len()) > self.limits.max_total_bytes {
            return Err(TransportError::Capacity);
        }
        let message = self
            .messages
            .entry(header.sequence)
            .or_insert_with(|| Partial {
                count: header.count,
                id: header.id,
                queue: header.queue,
                created_ms: now_ms,
                parts: BTreeMap::new(),
                bytes: 0,
            });
        self.bytes += fragment.data.len();
        message.bytes += fragment.data.len();
        message.parts.insert(header.index, fragment.data);
        if message.parts.len() != usize::from(message.count) {
            return Ok(None);
        }
        let message = self
            .messages
            .remove(&header.sequence)
            .ok_or(TransportError::InvalidFragment)?;
        self.bytes -= message.bytes;
        let mut output = Vec::with_capacity(message.bytes);
        for bytes in message.parts.into_values() {
            output.extend_from_slice(&bytes);
        }
        if output.len() < 4 {
            return Err(TransportError::InvalidFragment);
        }
        Ok(Some(output))
    }
}
