//! Online routine save ownership. Capture, queued write and uncertain outcome
//! remain distinct; no database acknowledgment is inferred from queue admission.
use crate::{
    saves::{SaveFailure, SaveHandle, SaveTicket, WriteOutcome},
    simulation::SimulationInput,
};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::{
    CharacterLease, DirtySaves, OwnershipState, SAVE_INTERVAL, SaveAck, SaveSnapshot,
};
use bace_simulation::{Command, PlayerSnapshotOutcome, PlayerSnapshotRequest};
use bace_storage_codec::PlayerSaveV6;
use std::{collections::BTreeMap, sync::mpsc::TrySendError, time::Duration};
use tokio::time::Instant;
mod inventory;
mod resolution;
pub use resolution::{RoutineResolutionOutcome, RoutineResolutionRequest};
mod writes;
use bace_persistence::OwnedSaveBatch;
use bace_storage_codec::ItemSaveV4;
struct Item {
    owner: u32,
    saved: ItemSaveV4,
    source_destination: Option<u8>,
    version: i64,
}
struct Player {
    detached: bool,
    binding: CharacterBinding,
    lease: CharacterLease,
    saved: PlayerSaveV6,
    version: i64,
    last_capture: Duration,
    first_dirty: Option<Duration>,
    reserved: bool,
    next_attempt: Duration,
    notice: u64,
    captured_notice: u64,
    requested_notice: u64,
    critical_notice: u64,
    drain_at: Option<Duration>,
}
struct PendingWrite {
    batch: OwnedSaveBatch,
    ticket: Option<SaveTicket>,
    uncertain: bool,
}
/// One application owner; methods never run on the simulation thread. Retain this
/// value through shutdown until `requires_drain` is false.
pub struct OnlinePlayerSaveService {
    players: BTreeMap<u32, Player>,
    dirty: DirtySaves,
    items: BTreeMap<u32, Item>,
    writes: BTreeMap<u32, PendingWrite>,
    capture: Option<(u64, u32, Duration)>,
    next: u64,
    origin: Instant,
    capacity: usize,
    byte_limit: usize,
    degraded: Option<String>,
    drain_at: Option<Duration>,
    poll_cursor: u32,
}
impl OnlinePlayerSaveService {
    pub fn new(capacity: usize, byte_limit: usize, origin: Instant) -> Result<Self, String> {
        if !(1..=4096).contains(&capacity) || !(1..=64 * 1024 * 1024).contains(&byte_limit) {
            return Err("online save capacity".into());
        }
        Ok(Self {
            players: BTreeMap::new(),
            dirty: DirtySaves::with_byte_limit(65536, byte_limit),
            items: BTreeMap::new(),
            writes: BTreeMap::new(),
            capture: None,
            next: 0,
            origin,
            capacity,
            byte_limit,
            degraded: None,
            drain_at: None,
            poll_cursor: 0,
        })
    }
    pub fn register(
        &mut self,
        binding: CharacterBinding,
        lease: CharacterLease,
        saved: PlayerSaveV6,
        version: i64,
        now: Duration,
    ) -> Result<(), String> {
        let id = binding.actor.0;
        let bytes = saved.encode().map_err(|e| e.to_string())?;
        if self.drain_at.is_some()
            || self.players.len() >= self.capacity
            || self.players.contains_key(&id)
            || self.items.contains_key(&id)
            || id != lease.character_id
            || id != saved.player.entity.object_id
            || binding.account.0 != saved.player.account_id
            || lease.state != OwnershipState::Online
            || version <= 0
        {
            return Err("online save registration identity/capacity".into());
        }
        self.dirty
            .register_clean(SaveSnapshot {
                object_id: id,
                mutation_revision: saved.player.entity.mutation_revision,
                expected_version: version,
                bytes,
            })
            .map_err(|e| e.to_string())?;
        self.players.insert(
            id,
            Player {
                detached: false,
                binding,
                lease,
                saved,
                version,
                last_capture: now,
                first_dirty: None,
                reserved: false,
                next_attempt: now,
                notice: 0,
                captured_notice: 0,
                requested_notice: 0,
                critical_notice: 0,
                drain_at: None,
            },
        );
        Ok(())
    }
    /// Optional early mutation notice. Periodic captures also detect changes; repeated
    /// notices preserve the earliest unsaved age.
    pub fn mark_dirty(&mut self, actor: u32, now: Duration) -> Result<(), String> {
        let player = self
            .players
            .get_mut(&actor)
            .ok_or("unknown online save actor")?;
        player.notice = player
            .notice
            .checked_add(1)
            .ok_or("dirty notice exhausted")?;
        player.first_dirty.get_or_insert(now);
        Ok(())
    }
    pub fn baseline(&self, actor: u32) -> Option<(&PlayerSaveV6, i64, CharacterLease)> {
        self.players
            .get(&actor)
            .map(|p| (&p.saved, p.version, p.lease))
    }
    /// Oldest pending capture first. A single outstanding capture bounds immutable
    /// snapshots and leaves the common output lane available for critical captures.
    pub fn request_capture(
        &mut self,
        input: &SimulationInput,
        correlation: u64,
        now: Duration,
        draining: bool,
    ) -> Result<bool, String> {
        if self.capture.is_some() {
            return Ok(false);
        }
        if draining {
            self.drain_at.get_or_insert(now);
        }
        let selected = self
            .players
            .iter()
            .filter(|(id, p)| !p.detached && !p.reserved && !self.writes.contains_key(id))
            .filter(|(_, p)| {
                now >= p.next_attempt
                    && (self
                        .drain_at
                        .or(p.drain_at)
                        .is_some_and(|target| p.last_capture < target)
                        || now
                            >= p.first_dirty
                                .unwrap_or(p.last_capture)
                                .saturating_add(SAVE_INTERVAL))
            })
            .min_by_key(|(id, p)| (p.first_dirty.unwrap_or(p.last_capture), **id))
            .map(|(&id, p)| (id, p.binding));
        let Some((id, binding)) = selected else {
            return Ok(false);
        };
        if correlation <= self.next {
            return Err("snapshot correlation is stale".into());
        }
        match input.try_submit(Command::PlayerSnapshot(PlayerSnapshotRequest {
            correlation,
            binding,
            operation: None,
        })) {
            Ok(()) => {
                let p = self.players.get_mut(&id).expect("selected player");
                p.requested_notice = p.notice;
                self.next = correlation;
                self.capture = Some((correlation, id, now));
                Ok(true)
            }
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Disconnected(_)) => Err("simulation snapshot ingress closed".into()),
        }
    }
    /// Unrelated critical captures are returned intact to their owning operation.
    pub fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix_millis: u64,
    ) -> Result<(), (String, PlayerSnapshotOutcome)> {
        let Some((token, id, now)) = self.capture else {
            return Err(("no routine capture pending".into(), outcome));
        };
        if token != outcome.correlation {
            return Err(("unrelated snapshot correlation".into(), outcome));
        }
        let result = (|| -> Result<(), String> {
            let p = self.players.get_mut(&id).ok_or("capture actor missing")?;
            let snapshot = outcome
                .result
                .as_ref()
                .map_err(|e| format!("routine capture deferred: {e:?}"))?;
            if snapshot.binding() != p.binding || snapshot.operation().is_some() {
                return Err("routine capture identity".into());
            }
            let mut requests = Vec::new();
            if snapshot.character().progression().revision()
                != p.saved.player.entity.mutation_revision
            {
                let next =
                    crate::player_saves::freeze_player_snapshot(&p.saved, snapshot, unix_millis)
                        .map_err(|e| e.to_string())?;
                requests.push(SaveSnapshot {
                    object_id: id,
                    mutation_revision: next.player.entity.mutation_revision,
                    expected_version: p.version,
                    bytes: next.encode().map_err(|e| e.to_string())?,
                });
            }
            requests.extend(inventory::freeze_items(id, &self.items, snapshot)?);
            let since = p.first_dirty.unwrap_or(p.last_capture);
            if requests.is_empty() && p.notice == p.requested_notice {
                p.first_dirty = None;
            }
            // The capture is immutable and all rows are validated before admission.
            self.dirty
                .mark_batch_at(requests, since)
                .map_err(|e| e.to_string())?;
            p.last_capture = now;
            p.captured_notice = p.requested_notice;
            Ok(())
        })();
        self.capture = None;
        self.players
            .get_mut(&id)
            .expect("pending player")
            .next_attempt = now.saturating_add(Duration::from_millis(100));
        if let Err(error) = result {
            let p = self.players.get_mut(&id).expect("pending player");
            p.first_dirty.get_or_insert(p.last_capture);
            self.degraded = Some(error.clone());
            return Err((error, outcome));
        }
        Ok(())
    }
    /// Reserve all participants together only after previous routine outcomes are known.
    /// Dirty baselines are allowed: the critical operation must capture its complete
    /// current-before state and persist it together with its after patch.
    pub fn critical_ready(&self, actors: &[u32]) -> Result<bool, String> {
        if actors.is_empty()
            || actors
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != actors.len()
        {
            return Err("critical participant bounds".into());
        }
        for id in actors {
            let p = self.players.get(id).ok_or("unknown critical player")?;
            if p.reserved {
                return Err("player already reserved for critical operation".into());
            }
        }
        Ok(!actors.iter().any(|id| {
            self.writes.contains_key(id) || self.capture.is_some_and(|(_, actor, _)| actor == *id)
        }))
    }
    pub fn begin_critical(&mut self, actors: &[u32]) -> Result<(), String> {
        if !self.critical_ready(actors)? {
            return Err("preceding routine save unresolved".into());
        }
        let ids: Vec<_> = actors
            .iter()
            .copied()
            .chain(
                self.items
                    .iter()
                    .filter(|(_, i)| actors.contains(&i.owner))
                    .map(|(&id, _)| id),
            )
            .collect();
        self.dirty.reserve(&ids).map_err(|e| e.to_string())?;
        for id in actors {
            self.players
                .get_mut(id)
                .expect("dirty registration owns player")
                .reserved = true;
            let p = self.players.get_mut(id).expect("validated player");
            p.critical_notice = p.notice;
        }
        Ok(())
    }
    /// Supply exact committed snapshots with their NEW durable versions.
    pub fn finish_critical(&mut self, committed: &[SaveSnapshot]) -> Result<(), String> {
        self.finish_critical_rows(committed)
    }
    /// Explicit reserved player roots permit item-only operations to complete
    /// without rewriting an unchanged player or inventing a dirty revision.
    pub fn finish_critical_for(
        &mut self,
        actors: &[u32],
        committed: &[SaveSnapshot],
    ) -> Result<(), String> {
        self.finish_critical_owner_rows(actors, committed)
    }
    pub fn cancel_critical(&mut self, actors: &[u32]) -> Result<(), String> {
        if actors.is_empty()
            || actors
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != actors.len()
        {
            return Err("critical participant bounds".into());
        }
        if actors
            .iter()
            .any(|id| !self.players.get(id).is_some_and(|p| p.reserved))
        {
            return Err("unknown critical player reservation".into());
        }
        let ids: Vec<_> = actors
            .iter()
            .copied()
            .chain(
                self.items
                    .iter()
                    .filter(|(_, i)| actors.contains(&i.owner))
                    .map(|(&id, _)| id),
            )
            .collect();
        self.dirty
            .cancel_reserved(&ids)
            .map_err(|e| e.to_string())?;
        for id in actors {
            self.players.get_mut(id).expect("validated player").reserved = false;
        }
        Ok(())
    }
    /// Called after accepted gameplay is quiesced for this character; other
    /// players and new admissions keep running while this actor drains.
    pub fn drain_player(&mut self, actor: u32, now: Duration) -> Result<(), String> {
        self.players
            .get_mut(&actor)
            .ok_or("unknown online player")?
            .drain_at
            .get_or_insert(now);
        Ok(())
    }
    pub fn player_clean(&self, actor: u32) -> bool {
        self.players.get(&actor).is_some_and(|p| {
            p.first_dirty.is_none()
                && !p.reserved
                && self.capture.is_none_or(|(_, id, _)| id != actor)
                && !self.writes.contains_key(&actor)
                && p.drain_at
                    .or(self.drain_at)
                    .is_none_or(|target| p.last_capture >= target)
                && self.dirty.is_object_clean(actor)
                && self
                    .items
                    .iter()
                    .filter(|(_, i)| i.owner == actor)
                    .all(|(&id, _)| self.dirty.is_object_clean(id))
        })
    }
    pub fn forget_clean(&mut self, actor: u32) -> Result<(), String> {
        if !self.player_clean(actor) {
            return Err("online player drain required".into());
        }
        let mut ids: Vec<_> = self
            .items
            .iter()
            .filter(|(_, i)| i.owner == actor)
            .map(|(&id, _)| id)
            .collect();
        ids.push(actor);
        self.dirty
            .forget_clean_batch(&ids)
            .map_err(|e| e.to_string())?;
        for id in ids {
            self.items.remove(&id);
        }
        self.players.remove(&actor);
        Ok(())
    }
    pub fn requires_drain(&self) -> bool {
        self.capture.is_some()
            || !self.writes.is_empty()
            || !self.dirty.is_clean()
            || self.players.values().any(|p| {
                p.first_dirty.is_some()
                    || p.reserved
                    || self
                        .drain_at
                        .or(p.drain_at)
                        .is_some_and(|target| p.last_capture < target)
            })
    }
    pub fn degraded(&self) -> Option<&str> {
        self.degraded.as_deref()
    }
    pub fn oldest_dirty_age(&self, now: Duration) -> Duration {
        self.players
            .values()
            .filter_map(|p| p.first_dirty)
            .chain(
                self.players
                    .keys()
                    .chain(self.items.keys())
                    .filter_map(|id| self.dirty.dirty_since(*id)),
            )
            .map(|since| now.saturating_sub(since))
            .max()
            .unwrap_or_default()
    }
}
#[cfg(test)]
mod tests;

mod detached;
