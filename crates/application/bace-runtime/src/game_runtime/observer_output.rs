//! Exact known-audience fanout. Adapter queue admission is not reliable ownership.
use super::*;
use crate::visibility_service::VisibilityServiceError;
use bace_gameplay_api::CharacterBinding;
use bace_types::EntityId;
use std::sync::mpsc::TrySendError;

const PREFIX: u64 = 0x4f00_0000_0000_0000;
const MASK: u64 = 0xff00_0000_0000_0000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Recipient {
    key: SessionKey,
    binding: CharacterBinding,
}
struct Inflight {
    correlation: u64,
    end: usize,
}
struct Fanout {
    sequence: u64,
    actor: EntityId,
    audience: Vec<Recipient>,
    recipient: usize,
    offset: usize,
    inflight: Option<Inflight>,
    reset: Option<Recipient>,
    tick: u64,
    ready: Option<(NetworkCommand, usize)>,
}
pub(super) struct ObserverOutputRuntime {
    pending: Option<Fanout>,
    next: u64,
}
impl ObserverOutputRuntime {
    pub(super) fn new() -> Self {
        Self {
            pending: None,
            next: PREFIX,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub(super) fn holds(&self, actor: EntityId) -> bool {
        self.pending.as_ref().is_some_and(|p| p.actor == actor)
    }
    fn begin(
        &mut self,
        work: &rewards::RewardObserverWork,
        audience: Vec<Recipient>,
        reset: Option<Recipient>,
        tick: u64,
        capacity: usize,
        batch_bytes: usize,
    ) -> Result<(), String> {
        if self.pending.is_some()
            || work.sequence == 0
            || audience.len() > capacity
            || capacity > 4096
            || work.messages.len() > 4096
            || batch_bytes < 4
            || work
                .messages
                .iter()
                .any(|m| m.queue >= 12 || !(4..=batch_bytes).contains(&m.bytes.len()))
            || work
                .messages
                .iter()
                .try_fold(0usize, |n, m| n.checked_add(m.bytes.len()))
                .is_none_or(|n| n > 16 * 1024 * 1024)
            || audience.windows(2).any(|pair| pair[0].key >= pair[1].key)
        {
            return Err("observer fanout bounds or audience identity".into());
        }
        self.pending = Some(Fanout {
            sequence: work.sequence,
            actor: work.actor,
            audience,
            recipient: 0,
            offset: 0,
            inflight: None,
            reset,
            tick,
            ready: None,
        });
        Ok(())
    }
    fn skip_recipient(&mut self) -> Result<(), String> {
        let pending = self.pending.as_mut().ok_or("observer work missing")?;
        if pending.inflight.is_some() {
            return Err("observer admission still unresolved".into());
        }
        pending.recipient += 1;
        pending.offset = 0;
        pending.ready = None;
        Ok(())
    }
    fn acknowledge(
        &mut self,
        key: SessionKey,
        correlation: u64,
        accepted: bool,
        messages: usize,
    ) -> Result<bool, String> {
        let pending = self
            .pending
            .as_mut()
            .ok_or("unmatched observer reliable receipt retained")?;
        let recipient = pending
            .audience
            .get(pending.recipient)
            .ok_or("observer recipient receipt mismatch")?;
        let inflight = pending
            .inflight
            .as_ref()
            .filter(|i| i.correlation == correlation)
            .ok_or("observer correlation mismatch")?;
        if recipient.key != key {
            return Err("observer session generation mismatch".into());
        }
        if accepted {
            pending.offset = inflight.end;
        }
        pending.inflight = None;
        if !accepted || pending.offset == messages {
            pending.recipient += 1;
            pending.offset = 0;
        }
        Ok(!accepted)
    }
    fn batch(
        &mut self,
        work: &rewards::RewardObserverWork,
        max_bytes: usize,
    ) -> Result<Option<(NetworkCommand, usize)>, String> {
        let pending = self.pending.as_mut().ok_or("observer work missing")?;
        if pending.sequence != work.sequence || pending.actor != work.actor {
            return Err("observer work identity changed".into());
        }
        if pending.inflight.is_some() || pending.recipient == pending.audience.len() {
            return Ok(None);
        }
        if let Some(ready) = pending.ready.take() {
            return Ok(Some(ready));
        }
        if work.messages.is_empty() {
            return Ok(None);
        }
        let mut bytes = 0usize;
        let mut end = pending.offset;
        while let Some(message) = work.messages.get(end) {
            if end - pending.offset == 256 || message.bytes.len() > max_bytes.saturating_sub(bytes)
            {
                break;
            }
            bytes += message.bytes.len();
            end += 1;
        }
        if end == pending.offset {
            return Err("observer message exceeds reliable batch capacity".into());
        }
        self.next = self
            .next
            .checked_add(1)
            .filter(|id| id & MASK == PREFIX)
            .ok_or("observer correlation exhausted")?;
        Ok(Some((
            NetworkCommand::SendReliableBatch {
                key: pending.audience[pending.recipient].key,
                correlation: self.next,
                messages: work.messages[pending.offset..end]
                    .iter()
                    .map(|m| (m.queue, m.bytes.clone()))
                    .collect(),
            },
            end,
        )))
    }
}
impl GameRuntime {
    pub(super) fn poll_observer_outputs(&mut self) -> Result<(), String> {
        // Receipts drain even while a later owner's output is holding the fence.
        for _ in 0..self.limits.work_per_poll {
            let Some(index) = self
                .reliable_admissions
                .iter()
                .position(|(_, id, _)| id & MASK == PREFIX)
            else {
                break;
            };
            let (key, correlation, accepted) = self.reliable_admissions[index];
            let work = self
                .pending_reward_observers()
                .ok_or("observer receipt without retained work")?;
            if self.observer_output.pending.as_ref().is_none_or(|pending| {
                pending.sequence != work.sequence || pending.actor != work.actor
            }) {
                return Err("observer receipt work identity mismatch".into());
            }
            let count = work.messages.len();
            let rejected = self
                .observer_output
                .acknowledge(key, correlation, accepted, count)?;
            self.reliable_admissions.remove(index);
            if rejected && let Some(session) = self.sessions.get_mut(&key) {
                session.terminated = true;
                return Err("observer reliable admission rejected; generation terminated".into());
            }
        }
        if !self.network_output.is_empty() || self.portals.publication_pending() {
            return Ok(());
        }
        let max_bytes = self
            .network
            .maximum_message_bytes()
            .min(self.limits.message_bytes);
        if self.observer_output.pending.is_none() {
            if self.visibility.service.pending() {
                return Ok(());
            }
            let Some(work) = self.pending_reward_observers() else {
                return Ok(());
            };
            let mut audience = Vec::new();
            for key in self.visibility.service.observing_sessions(work.actor) {
                if self
                    .sessions
                    .get(&key)
                    .is_none_or(|s| s.terminated || s.closing || s.disconnected)
                {
                    continue;
                }
                if let Some(binding) = self.players.binding_for(key) {
                    if audience.len() == self.limits.sessions {
                        return Err("observer audience capacity".into());
                    }
                    audience.push(Recipient { key, binding });
                }
            }
            let reset = if work.reset_visibility {
                self.players
                    .entered_bindings()
                    .find(|(_, b)| b.actor == work.actor)
                    .map(|(key, binding)| Recipient { key, binding })
            } else {
                None
            };
            let tick = u64::try_from(self.last_elapsed.as_nanos() / 33_333_333)
                .map_err(|_| "observer reset clock overflow")?;
            // Borrow the original bytes; fanout stores only immutable recipient identities.
            self.observer_output.begin(
                self.rewards.observers.front().expect("checked work"),
                audience,
                reset,
                tick,
                self.limits.sessions,
                max_bytes,
            )?;
        }
        for _ in 0..self.limits.work_per_poll {
            let pending = self.observer_output.pending.as_mut().expect("started");
            if pending.inflight.is_some() {
                break;
            }
            let work = self
                .rewards
                .observers
                .front()
                .ok_or("retained observer work missing")?;
            if pending.sequence != work.sequence || pending.actor != work.actor {
                return Err("retained observer identity changed".into());
            }
            if let Some(recipient) = pending.audience.get(pending.recipient) {
                let live = self
                    .sessions
                    .get(&recipient.key)
                    .is_some_and(|s| !s.terminated && !s.closing && !s.disconnected)
                    && self.players.binding_for(recipient.key) == Some(recipient.binding);
                if !live || work.messages.is_empty() {
                    self.observer_output.skip_recipient()?;
                    continue;
                }
                let Some((command, end)) = self.observer_output.batch(work, max_bytes)? else {
                    break;
                };
                let NetworkCommand::SendReliableBatch { correlation, .. } = &command else {
                    unreachable!()
                };
                let correlation = *correlation;
                match self.network.try_send(command) {
                    Ok(()) => {
                        self.observer_output
                            .pending
                            .as_mut()
                            .expect("started")
                            .inflight = Some(Inflight { correlation, end })
                    }
                    Err(TrySendError::Full(command)) => {
                        self.observer_output
                            .pending
                            .as_mut()
                            .expect("started")
                            .ready = Some((command, end))
                    }
                    Err(TrySendError::Disconnected(command)) => {
                        self.observer_output
                            .pending
                            .as_mut()
                            .expect("started")
                            .ready = Some((command, end));
                        return Err("observer network closed; exact work retained".into());
                    }
                }
                break;
            }
            if let Some(recipient) = pending.reset {
                let live = self
                    .sessions
                    .get(&recipient.key)
                    .is_some_and(|s| !s.terminated && !s.closing && !s.disconnected)
                    && self.players.binding_for(recipient.key) == Some(recipient.binding);
                if live {
                    match self.visibility.service.reset_observer_event(
                        recipient.key,
                        pending.tick,
                        pending.sequence,
                    ) {
                        Ok(()) => {}
                        Err(VisibilityServiceError::Busy) => break,
                        Err(error) => {
                            return Err(format!("observer teleport reset retained: {error:?}"));
                        }
                    }
                }
            }
            let sequence = pending.sequence;
            self.acknowledge_reward_observers(sequence)?;
            self.observer_output.pending = None;
            break;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
