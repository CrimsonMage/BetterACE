//! Bounded ACE shard administration with explicit time and durable shutdown.
//! Source: AdminShardCommands.cs, ServerManager.cs and WorldManager.Open/Close at
//! official ACE 47edade3. The destructive five-minute forced-logoff failsafe is
//! intentionally replaced by retained drain requests until durability completes.
mod commands;
mod time;
mod types;
use bace_admin::PreparedShardCommand;
use bace_auth::AccessLevel;
use std::collections::VecDeque;
pub use types::*;

#[derive(Clone, Debug)]
struct State {
    interval: u32,
    world_open: bool,
    shutdown: ShardShutdown,
    deadline_unix: i64,
    last_notice: u64,
    last_clock: Option<u64>,
}
pub struct ShardControl {
    state: State,
    capacity: usize,
    sequence: u64,
    next_drain: u64,
    events: VecDeque<ShardEvent>,
    drains: VecDeque<ShardDrainRequest>,
}
impl ShardControl {
    /// Source default interval is60 seconds. Initial world status is supplied by
    /// startup policy (ACE begins Closed, then opens unless world_closed is set).
    pub fn new(capacity: usize, world_open: bool) -> Result<Self, ShardError> {
        if !(8..=4096).contains(&capacity) {
            return Err(ShardError::Capacity);
        }
        Ok(Self {
            state: State {
                interval: 60,
                world_open,
                shutdown: ShardShutdown::Idle,
                deadline_unix: time::MIN_UNIX,
                last_notice: 0,
                last_clock: None,
            },
            capacity,
            sequence: 0,
            next_drain: 0,
            events: VecDeque::new(),
            drains: VecDeque::new(),
        })
    }
    pub fn shutdown(&self) -> ShardShutdown {
        self.state.shutdown
    }
    pub fn interval(&self) -> u32 {
        self.state.interval
    }
    pub fn world_open(&self) -> bool {
        self.state.world_open
    }
    /// Admission is also closed during irreversible shutdown draining; otherwise
    /// new arrivals could indefinitely prevent a complete, durable drain.
    pub fn accepts_players(&self) -> bool {
        self.state.world_open
            && !matches!(
                self.state.shutdown,
                ShardShutdown::Draining(_) | ShardShutdown::Complete
            )
    }
    pub fn has_pending(&self) -> bool {
        !self.events.is_empty()
            || !self.drains.is_empty()
            || matches!(
                self.state.shutdown,
                ShardShutdown::Countdown { .. } | ShardShutdown::Draining(_)
            )
    }
    pub fn peek_event(&self) -> Option<&ShardEvent> {
        self.events.front()
    }
    /// Only acknowledge after the effect has entered its reliable output owner.
    pub fn acknowledge_event(&mut self, sequence: u64) -> Result<(), ShardError> {
        if self.events.front().map(|e| e.sequence) != Some(sequence) {
            return Err(ShardError::StaleReceipt);
        }
        self.events.pop_front();
        Ok(())
    }
    pub fn peek_drain(&self) -> Option<ShardDrainRequest> {
        self.drains.front().copied()
    }
    fn clock(&self, clock: ShardClock) -> Result<(), ShardError> {
        time::validate(clock)?;
        if self
            .state
            .last_clock
            .is_some_and(|last| clock.monotonic_millis < last)
        {
            return Err(ShardError::Clock);
        }
        Ok(())
    }
    /// Prepared input is not an authorization token. Supply the live session
    /// principal (or separately authenticated host identity) on every execution.
    pub fn apply(
        &mut self,
        command: &PreparedShardCommand,
        issuer: ShardIssuer<'_>,
        clock: ShardClock,
    ) -> Result<(), ShardError> {
        if let ShardIssuer::Player {
            name, principal, ..
        } = issuer
        {
            if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
                return Err(ShardError::InvalidInput);
            }
            if !principal.allows(AccessLevel::Admin, command.sudo()) {
                return Err(ShardError::NotAuthorized);
            }
        }
        self.clock(clock)?;
        let mut state = self.state.clone();
        let mut effects = Vec::new();
        let mut drains = Vec::new();
        commands::apply(
            &mut state,
            command.operation(),
            issuer,
            clock,
            &mut effects,
            &mut drains,
        )?;
        state.last_clock = Some(clock.monotonic_millis);
        self.commit(state, effects, drains)
    }
    /// O(1), allocation-free when no notice/transition is due. No catch-up flood:
    /// ACE's loop only emits when the current TimeSpan matches a notice window.
    pub fn advance(&mut self, clock: ShardClock) -> Result<(), ShardError> {
        self.clock(clock)?;
        let ShardShutdown::Countdown { deadline_millis } = self.state.shutdown else {
            self.state.last_clock = Some(clock.monotonic_millis);
            return Ok(());
        };
        let mut state = self.state.clone();
        let mut effects = Vec::new();
        let mut drains = Vec::new();
        if clock.monotonic_millis > deadline_millis {
            state.shutdown = ShardShutdown::Draining(ShardDrainKind::ShutdownPlayers);
            drains.push(ShardDrainKind::ShutdownPlayers);
        } else {
            let remaining = deadline_millis - clock.monotonic_millis + 1000;
            let seconds = remaining / 1000;
            // Source compares component text, so the day component is omitted.
            let component_seconds = seconds % 86400;
            if matches!(
                component_seconds,
                7200 | 3600 | 2700 | 1800 | 900 | 600 | 300 | 120 | 90 | 60 | 30 | 15 | 10 | 5
            ) && clock.monotonic_millis.saturating_sub(state.last_notice) > 2000
            {
                effects.push(ShardEffect::Broadcast {
                    text: time::notice("System", remaining, remaining <= 10_000),
                });
                state.last_notice = clock.monotonic_millis;
            }
        }
        state.last_clock = Some(clock.monotonic_millis);
        self.commit(state, effects, drains)
    }
    /// No timeout can satisfy this receipt. Each stage retains its request until
    /// its real owner reports zero remaining work, dirty state and operations.
    pub fn confirm_drain(&mut self, receipt: ShardDrainReceipt) -> Result<(), ShardError> {
        if self.drains.front().copied() != Some(receipt.request) {
            return Err(ShardError::StaleReceipt);
        }
        if receipt.remaining != 0 || receipt.unsaved != 0 || receipt.pending_operations != 0 {
            return Err(ShardError::DurabilityPending);
        }
        let next = match receipt.request.kind {
            ShardDrainKind::BootOrdinaryPlayers => None,
            ShardDrainKind::ShutdownPlayers => Some(ShardDrainKind::DisconnectSessions),
            ShardDrainKind::DisconnectSessions => Some(ShardDrainKind::UnloadRegions),
            ShardDrainKind::UnloadRegions => Some(ShardDrainKind::StopWorld),
            ShardDrainKind::StopWorld => None,
        };
        if next.is_some() && self.next_drain == u64::MAX {
            return Err(ShardError::Capacity);
        }
        self.drains.pop_front();
        if let Some(kind) = next {
            self.next_drain += 1;
            self.drains.push_back(ShardDrainRequest {
                id: self.next_drain,
                kind,
            });
            self.state.shutdown = ShardShutdown::Draining(kind);
        } else if receipt.request.kind == ShardDrainKind::StopWorld {
            self.state.shutdown = ShardShutdown::Complete;
        }
        Ok(())
    }
    fn commit(
        &mut self,
        state: State,
        effects: Vec<ShardEffect>,
        drains: Vec<ShardDrainKind>,
    ) -> Result<(), ShardError> {
        if self.events.len() + effects.len() > self.capacity
            || self.drains.len() + drains.len() > self.capacity
            || self.sequence.checked_add(effects.len() as u64).is_none()
            || self.next_drain.checked_add(drains.len() as u64).is_none()
        {
            return Err(ShardError::Capacity);
        }
        self.state = state;
        for effect in effects {
            self.sequence += 1;
            self.events.push_back(ShardEvent {
                sequence: self.sequence,
                effect,
            });
        }
        for kind in drains {
            self.next_drain += 1;
            self.drains.push_back(ShardDrainRequest {
                id: self.next_drain,
                kind,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
