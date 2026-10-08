//! Canonical private reward projection and retained observer routing obligations.
use super::*;
use crate::reward_service::{RewardService, project_reward_for_player};
use bace_replication::{BatchLimits, ReplicationMessage};
pub struct RewardObserverWork {
    pub sequence: u64,
    pub actor: bace_types::EntityId,
    pub messages: Vec<ReplicationMessage>,
    pub reset_visibility: bool,
}
pub(super) struct RewardRuntime {
    service: RewardService,
    pub(super) observers: VecDeque<RewardObserverWork>,
    bytes: usize,
    next: u64,
}
impl RewardRuntime {
    pub(super) fn new() -> Self {
        Self {
            service: RewardService::new(),
            observers: VecDeque::new(),
            bytes: 0,
            next: 0,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.service.has_pending() || !self.observers.is_empty()
    }
}
impl GameRuntime {
    pub(super) fn observer_holds(&self, actor: bace_types::EntityId) -> bool {
        self.rewards
            .observers
            .iter()
            .any(|work| work.actor == actor)
            || self.observer_output.holds(actor)
    }
    pub(super) fn retain_observer_messages(
        &mut self,
        batches: Vec<(bace_types::EntityId, Vec<ReplicationMessage>)>,
    ) -> Result<(), String> {
        let bytes = batches
            .iter()
            .flat_map(|(_, m)| m)
            .map(|m| m.bytes.len())
            .sum::<usize>();
        if batches.len() > 256 - self.rewards.observers.len()
            || bytes > 16 * 1024 * 1024 - self.rewards.bytes
            || self
                .rewards
                .next
                .checked_add(batches.len() as u64)
                .is_none()
        {
            return Err("observer routing capacity; accepted event retained".into());
        }
        for (actor, messages) in batches {
            self.rewards.next += 1;
            self.rewards.observers.push_back(RewardObserverWork {
                sequence: self.rewards.next,
                actor,
                messages,
                reset_visibility: false,
            });
        }
        self.rewards.bytes += bytes;
        Ok(())
    }
    pub(super) fn observer_room(&self, count: usize, bytes: usize) -> bool {
        count <= 256 - self.rewards.observers.len()
            && bytes <= 16 * 1024 * 1024 - self.rewards.bytes
            && self.rewards.next.checked_add(count as u64).is_some()
    }
    pub(super) fn mark_last_observer_visibility_reset(&mut self) {
        self.rewards
            .observers
            .back_mut()
            .expect("inserted observer work")
            .reset_visibility = true;
    }
    pub fn pending_reward_observers(&self) -> Option<&RewardObserverWork> {
        self.rewards.observers.front()
    }
    /// Acknowledge only once the accepted visibility router owns the complete
    /// message batch. No recipient is inferred from online session membership.
    pub fn acknowledge_reward_observers(&mut self, sequence: u64) -> Result<(), String> {
        if self
            .rewards
            .observers
            .front()
            .is_none_or(|v| v.sequence != sequence)
        {
            return Err("reward observer correlation".into());
        }
        let work = self
            .rewards
            .observers
            .pop_front()
            .expect("matched obligation");
        self.rewards.bytes -= work.messages.iter().map(|m| m.bytes.len()).sum::<usize>();
        Ok(())
    }
    pub(super) fn poll_reward_outputs(&mut self) -> Result<(), String> {
        let limits = BatchLimits {
            max_messages: self.limits.messages,
            max_bytes: self.limits.message_bytes,
            max_message_bytes: self.limits.message_bytes,
            max_string_bytes: 4096,
        };
        let rewards = &mut self.rewards;
        let players = &mut self.players;
        let network = &self.network;
        rewards.service.pump(
            &self.simulation,
            self.limits.work_per_poll,
            |event| project_reward_for_player(players, event, limits),
            |projection| {
                if projection.observers.is_empty() {
                    return Ok(vec![]);
                }
                let bytes = projection
                    .observers
                    .iter()
                    .map(|m| m.bytes.len())
                    .sum::<usize>();
                if rewards.observers.len() >= 256 || bytes > 16 * 1024 * 1024 - rewards.bytes {
                    return Err(
                        "reward observer routing capacity; exact projection retained".into(),
                    );
                }
                rewards.next = rewards
                    .next
                    .checked_add(1)
                    .ok_or("reward observer sequence overflow")?;
                rewards.observers.push_back(RewardObserverWork {
                    sequence: rewards.next,
                    actor: projection.actor,
                    messages: projection.observers.clone(),
                    reset_visibility: false,
                });
                rewards.bytes += bytes;
                Ok(vec![])
            },
            |command| {
                network.try_send(command).map_err(|e| match e {
                    std::sync::mpsc::TrySendError::Full(c)
                    | std::sync::mpsc::TrySendError::Disconnected(c) => c,
                })
            },
        )?;
        Ok(())
    }
}
