// Scheduling derived from ACE NetworkSession.SendBundle and MessageFragment at
// 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only.
use crate::{TransportError, fragment_message};
use bace_wire::{Fragment, SERVER_BODY_LIMIT};
use std::collections::VecDeque;

struct Message {
    parts: VecDeque<Fragment>,
    remaining: usize,
    tail_sent: bool,
}
impl Message {
    fn take(&mut self, tail: bool) -> Option<Fragment> {
        let part = if tail {
            self.tail_sent = true;
            self.parts.pop_back()
        } else {
            self.parts.pop_front()
        }?;
        self.remaining -= part.data.len();
        Some(part)
    }
}

/// Per-peer bounded outgoing messages. Queue 9 preserves UI message order.
/// Message sequence numbers are assigned when a queue's bundle starts, as in ACE.
pub struct Bundler {
    queues: [VecDeque<Vec<u8>>; 12],
    active: Vec<Message>,
    active_queue: usize,
    next_queue: usize,
    next_sequence: u32,
    bytes: usize,
    messages: usize,
    max_bytes: usize,
    max_messages: usize,
    max_message: usize,
}
impl Bundler {
    pub fn new(max_messages: usize, max_bytes: usize, max_message: usize) -> Self {
        Self {
            queues: std::array::from_fn(|_| VecDeque::new()),
            active: Vec::new(),
            active_queue: 0,
            next_queue: 0,
            next_sequence: 0,
            bytes: 0,
            messages: 0,
            max_bytes,
            max_messages,
            max_message,
        }
    }
    pub fn enqueue(&mut self, queue: u16, bytes: Vec<u8>) -> Result<(), TransportError> {
        if queue >= 12 || bytes.len() < 4 || bytes.len() > self.max_message {
            return Err(TransportError::InvalidFragment);
        }
        if self.messages >= self.max_messages
            || self.bytes.saturating_add(bytes.len()) > self.max_bytes
        {
            return Err(TransportError::Capacity);
        }
        self.bytes += bytes.len();
        self.messages += 1;
        self.queues[usize::from(queue)].push_back(bytes);
        Ok(())
    }
    pub fn buffered_bytes(&self) -> usize {
        self.bytes
    }
    pub fn pending_messages(&self) -> usize {
        self.messages
    }
    pub fn has_active_bundle(&self) -> bool {
        !self.active.is_empty()
    }
    /// Starts one bundle using rotating queue selection to prevent starvation.
    pub fn start_bundle(&mut self) -> Result<bool, TransportError> {
        if self.has_active_bundle() {
            return Ok(true);
        }
        let Some(queue) = (0..12)
            .map(|offset| (self.next_queue + offset) % 12)
            .find(|&queue| !self.queues[queue].is_empty())
        else {
            return Ok(false);
        };
        let count = u32::try_from(self.queues[queue].len())
            .map_err(|_| TransportError::SequenceExhausted)?;
        self.next_sequence
            .checked_add(count)
            .ok_or(TransportError::SequenceExhausted)?;
        self.active_queue = queue;
        self.next_queue = (queue + 1) % 12;
        while let Some(bytes) = self.queues[queue].pop_front() {
            let parts =
                fragment_message(self.next_sequence, queue as u16, &bytes, self.max_message)?;
            self.next_sequence += 1;
            self.active.push(Message {
                tail_sent: parts.len() == 1,
                remaining: bytes.len(),
                parts: parts.into(),
            });
        }
        Ok(true)
    }
    /// Produces at most 464 body bytes, preferring a large leading message,
    /// then packing tails/small messages. Optional headers are handled by Peer
    /// on control queue packets, separately from application bundles.
    pub fn next_fragments(&mut self) -> Vec<Fragment> {
        let mut result = Vec::new();
        let mut space = SERVER_BODY_LIMIT;
        if let Some(first) = self.active.first_mut()
            && first.remaining >= SERVER_BODY_LIMIT
        {
            if let Some(part) = first.take(false) {
                self.bytes -= part.data.len();
                result.push(part);
            }
        } else {
            for message in &mut self.active {
                let tail = !message.tail_sent
                    && message
                        .parts
                        .back()
                        .is_some_and(|part| usize::from(part.header.size) <= space);
                let fits = tail
                    || message
                        .parts
                        .front()
                        .is_some_and(|part| usize::from(part.header.size) <= space);
                if fits {
                    if let Some(part) = message.take(tail) {
                        space -= usize::from(part.header.size);
                        self.bytes -= part.data.len();
                        result.push(part);
                    }
                } else if self.active_queue == 9 {
                    break;
                }
            }
        }
        self.active.retain(|message| {
            if message.parts.is_empty() {
                self.messages -= 1;
                false
            } else {
                true
            }
        });
        result
    }
}
