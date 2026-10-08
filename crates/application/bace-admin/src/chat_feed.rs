//! Bounded live chat history. Persistence and private conversations are excluded.
use bace_config::ChatApiConfig;
use bace_gameplay_api::social::{AcceptedChat, ChatChannel};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedChannel {
    General,
    Trade,
    Audit,
}
impl FeedChannel {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::General => 0,
            Self::Trade => 1,
            Self::Audit => 2,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Trade => "trade",
            Self::Audit => "audit",
        }
    }
    pub(crate) fn from_chat(channel: ChatChannel) -> Option<Self> {
        match channel {
            ChatChannel::General => Some(Self::General),
            ChatChannel::Trade => Some(Self::Trade),
            ChatChannel::Audit => Some(Self::Audit),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct FeedEvent {
    pub sequence: u64,
    pub accepted_sequence: u64,
    pub unix_seconds: i64,
    pub sender: u32,
    pub sender_name: String,
    pub text: String,
}
impl FeedEvent {
    fn bytes(&self) -> usize {
        self.sender_name.len() + self.text.len() + 64
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct FeedBatch {
    pub channel: FeedChannel,
    pub events: Vec<FeedEvent>,
    pub cursor: String,
    pub gap: bool,
    pub reset: bool,
    pub dropped_publications: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedError {
    Invalid,
    PrivateChannel,
    Full,
    Closed,
    Poisoned,
    Overflow,
}
struct ChannelHistory {
    events: VecDeque<FeedEvent>,
    bytes: usize,
}
struct FeedState {
    channels: [ChannelHistory; 3],
}
struct Shared {
    epoch: String,
    config: ChatApiConfig,
    state: Mutex<FeedState>,
    dropped: [AtomicU64; 3],
}
#[derive(Clone)]
pub struct ChatFeed {
    shared: Arc<Shared>,
}
#[derive(Clone)]
pub struct ChatPublisher {
    sender: mpsc::Sender<(FeedChannel, FeedEvent)>,
    sequence: Arc<[AtomicU64; 3]>,
    shared: Arc<Shared>,
}
pub struct ChatConsumer {
    receiver: mpsc::Receiver<(FeedChannel, FeedEvent)>,
    feed: ChatFeed,
}
impl ChatFeed {
    pub fn new(config: ChatApiConfig) -> Result<(Self, ChatPublisher, ChatConsumer), FeedError> {
        config.validate().map_err(|_| FeedError::Invalid)?;
        let mut epoch = [0u8; 16];
        OsRng
            .try_fill_bytes(&mut epoch)
            .map_err(|_| FeedError::Invalid)?;
        let epoch = epoch.iter().map(|b| format!("{b:02x}")).collect();
        let (sender, receiver) = mpsc::channel(config.publication_capacity);
        let shared = Arc::new(Shared {
            epoch,
            config,
            state: Mutex::new(FeedState {
                channels: std::array::from_fn(|_| ChannelHistory {
                    events: VecDeque::new(),
                    bytes: 0,
                }),
            }),
            dropped: std::array::from_fn(|_| AtomicU64::new(0)),
        });
        let feed = Self {
            shared: shared.clone(),
        };
        let publisher = ChatPublisher {
            sender,
            sequence: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
            shared,
        };
        Ok((feed.clone(), publisher, ChatConsumer { receiver, feed }))
    }
    pub fn config(&self) -> &ChatApiConfig {
        &self.shared.config
    }
    pub fn read(
        &self,
        channel: FeedChannel,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<FeedBatch, FeedError> {
        if limit == 0 || limit > self.shared.config.max_batch {
            return Err(FeedError::Invalid);
        }
        let mut after = 0;
        let mut seen_dropped = 0;
        let mut reset = false;
        if let Some(cursor) = cursor {
            if cursor.len() > 128 {
                return Err(FeedError::Invalid);
            }
            let parts: Vec<_> = cursor.split(':').collect();
            if parts.len() != 4 || parts[1] != channel.name() {
                return Err(FeedError::Invalid);
            }
            after = parts[2].parse().map_err(|_| FeedError::Invalid)?;
            seen_dropped = parts[3].parse().map_err(|_| FeedError::Invalid)?;
            if parts[0] != self.shared.epoch {
                reset = true;
                after = 0;
            }
        }
        let state = self.shared.state.lock().map_err(|_| FeedError::Poisoned)?;
        let ring = &state.channels[channel.index()];
        let last = ring.events.back().map_or(0, |e| e.sequence);
        if !reset && after > last {
            return Err(FeedError::Invalid);
        }
        let dropped = self.shared.dropped[channel.index()].load(Ordering::Relaxed);
        let mut gap = reset
            || dropped > seen_dropped
            || ring
                .events
                .front()
                .is_some_and(|e| after.saturating_add(1) < e.sequence);
        let events: Vec<_> = ring
            .events
            .iter()
            .filter(|e| e.sequence > after)
            .take(limit)
            .cloned()
            .collect();
        for event in &events {
            if event.sequence != after.saturating_add(1) {
                gap = true;
            }
            after = event.sequence;
        }
        Ok(FeedBatch {
            channel,
            events,
            cursor: format!("{}:{}:{after}:{dropped}", self.shared.epoch, channel.name()),
            gap,
            reset,
            dropped_publications: dropped,
        })
    }
    fn append(&self, channel: FeedChannel, event: FeedEvent) -> Result<(), FeedError> {
        let mut state = self.shared.state.lock().map_err(|_| FeedError::Poisoned)?;
        let ring = &mut state.channels[channel.index()];
        if ring
            .events
            .back()
            .is_some_and(|last| last.sequence >= event.sequence)
        {
            return Err(FeedError::Invalid);
        }
        while !ring.events.is_empty()
            && (ring.events.len() >= self.shared.config.events_per_channel
                || ring.bytes + event.bytes() > self.shared.config.bytes_per_channel)
        {
            ring.bytes -= ring.events.pop_front().ok_or(FeedError::Invalid)?.bytes();
        }
        ring.bytes += event.bytes();
        ring.events.push_back(event);
        Ok(())
    }
}
impl ChatPublisher {
    /// Called by the single simulation owner, never waits for HTTP or a mutex.
    /// Full queues are explicitly counted; channel sequence gaps remain visible.
    pub fn publish(&self, event: &AcceptedChat) -> Result<(), FeedError> {
        let channel = self.validate(event)?;
        let sequence = self.sequence[channel.index()]
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| FeedError::Overflow)?
            + 1;
        let output = Self::output(event, sequence);
        self.sender.try_send((channel, output)).map_err(|error| {
            self.shared.dropped[channel.index()].fetch_add(1, Ordering::Relaxed);
            match error {
                mpsc::error::TrySendError::Full(_) => FeedError::Full,
                mpsc::error::TrySendError::Closed(_) => FeedError::Closed,
            }
        })
    }

    /// Audit from a committed game action must not consume a feed sequence or
    /// count a drop when bounded publication is full. The caller retains and
    /// retries the exact accepted event before advancing in-game recipients.
    pub fn try_publish_audit(&self, event: &AcceptedChat) -> Result<(), FeedError> {
        if event.channel != ChatChannel::Audit {
            return Err(FeedError::PrivateChannel);
        }
        let channel = self.validate(event)?;
        let permit = self.sender.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(()) => FeedError::Full,
            mpsc::error::TrySendError::Closed(()) => FeedError::Closed,
        })?;
        let sequence = self.sequence[channel.index()]
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| FeedError::Overflow)?
            + 1;
        permit.send((channel, Self::output(event, sequence)));
        Ok(())
    }

    fn validate(&self, event: &AcceptedChat) -> Result<FeedChannel, FeedError> {
        let channel = FeedChannel::from_chat(event.channel).ok_or(FeedError::PrivateChannel)?;
        if event.sender_name.len() > 1024
            || event.text.len() > 16384
            || event.sender_name.len() + event.text.len() + 64
                > self.shared.config.bytes_per_channel
        {
            return Err(FeedError::Invalid);
        }
        Ok(channel)
    }

    fn output(event: &AcceptedChat, sequence: u64) -> FeedEvent {
        FeedEvent {
            sequence,
            accepted_sequence: event.sequence,
            unix_seconds: event.unix_seconds,
            sender: event.sender.0,
            sender_name: event.sender_name.clone(),
            text: event.text.clone(),
        }
    }
}
impl ChatConsumer {
    /// Close publication admission on shutdown, then drain the already accepted bounded queue.
    pub async fn run_until(
        mut self,
        mut stop: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(), FeedError> {
        loop {
            tokio::select! {
                biased;
                _ = &mut stop => { self.receiver.close(); break; },
                next = self.receiver.recv() => match next {
                    Some((channel,event)) => self.feed.append(channel,event)?,
                    None => return Ok(()),
                }
            }
        }
        while let Some((channel, event)) = self.receiver.recv().await {
            self.feed.append(channel, event)?;
        }
        Ok(())
    }

    pub async fn run(mut self) -> Result<(), FeedError> {
        while let Some((channel, event)) = self.receiver.recv().await {
            self.feed.append(channel, event)?;
        }
        Ok(())
    }
    /// Bounded adapter/test worker pass; never call this on the simulation owner.
    pub fn drain(&mut self, maximum: usize) -> Result<usize, FeedError> {
        if maximum > 4096 {
            return Err(FeedError::Invalid);
        }
        let mut count = 0;
        while count < maximum {
            match self.receiver.try_recv() {
                Ok((channel, event)) => {
                    self.feed.append(channel, event)?;
                    count += 1;
                }
                Err(_) => break,
            }
        }
        Ok(count)
    }
}
