use crate::{SaveAck, SaveSnapshot};
use std::{collections::BTreeMap, time::Duration};

pub const SAVE_INTERVAL: Duration = Duration::from_secs(5);
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DirtyError {
    #[error("dirty-save capacity reached")]
    Capacity,
    #[error("unknown object")]
    Unknown,
    #[error("object is reserved or already in flight")]
    Busy,
    #[error("revision is stale or acknowledgment does not match the in-flight save")]
    Stale,
}
#[derive(Debug)]
struct InFlight {
    mutation_revision: u64,
    expected_version: i64,
    bytes: usize,
}
#[derive(Debug)]
struct Entry {
    snapshot: SaveSnapshot,
    in_flight: Option<InFlight>,
    dirty: bool,
    reserved: bool,
    dirty_since: Option<Duration>,
    dirty_order: u64,
    after_dispatch: Option<(Duration, u64)>,
}
/// A simulation-owned bounded coordinator. Time is supplied by the caller.
/// Reservation of valuable operations is deliberately outside routine batching.
#[derive(Debug)]
pub struct DirtySaves {
    entries: BTreeMap<u32, Entry>,
    capacity: usize,
    byte_limit: usize,
    last_now: Duration,
    next_order: u64,
}
impl DirtySaves {
    pub fn new(capacity: usize) -> Self {
        Self::with_byte_limit(capacity, 64 * 1024 * 1024)
    }
    /// Independently bounds retained latest state and outstanding routine request bytes.
    pub fn with_byte_limit(capacity: usize, byte_limit: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity,
            byte_limit,
            last_now: Duration::ZERO,
            next_order: 0,
        }
    }
    /// Seed an already durable aggregate without fabricating a save acknowledgment.
    pub fn register_clean(&mut self, snapshot: SaveSnapshot) -> Result<(), DirtyError> {
        if self.entries.contains_key(&snapshot.object_id) || snapshot.expected_version <= 0 {
            return Err(DirtyError::Stale);
        }
        let id = snapshot.object_id;
        self.mark_at(snapshot, self.last_now)?;
        let entry = self.entries.get_mut(&id).ok_or(DirtyError::Unknown)?;
        entry.dirty = false;
        entry.dirty_since = None;
        Ok(())
    }
    /// Coalesce a newer immutable state. Caller maintains monotonic mutation revisions.
    pub fn mark(&mut self, snapshot: SaveSnapshot) -> Result<(), DirtyError> {
        self.mark_at(snapshot, self.last_now)
    }
    /// Explicit monotonic simulation time; coalescing never postpones the first unsaved deadline.
    pub fn mark_at(&mut self, snapshot: SaveSnapshot, now: Duration) -> Result<(), DirtyError> {
        self.last_now = self.last_now.max(now);
        let order = self.next_order;
        self.next_order = self.next_order.checked_add(1).ok_or(DirtyError::Stale)?;
        if snapshot.bytes.len() > 17 * 1024 * 1024 || snapshot.expected_version < 0 {
            return Err(DirtyError::Capacity);
        }
        let replaced = self
            .entries
            .get(&snapshot.object_id)
            .map_or(0, |e| e.snapshot.bytes.len());
        let projected = self
            .retained_bytes()
            .saturating_sub(replaced)
            .saturating_add(snapshot.bytes.len());
        if projected > self.byte_limit {
            return Err(DirtyError::Capacity);
        }
        if let Some(entry) = self.entries.get_mut(&snapshot.object_id) {
            if entry.reserved {
                return Err(DirtyError::Busy);
            }
            if snapshot.mutation_revision <= entry.snapshot.mutation_revision {
                return Err(DirtyError::Stale);
            }
            // Only acknowledgments may change the tracked durable version.
            if snapshot.expected_version != entry.snapshot.expected_version {
                return Err(DirtyError::Stale);
            }
            if !entry.dirty {
                entry.dirty_since = Some(now);
                entry.dirty_order = order;
            }
            if entry.in_flight.is_some() && entry.after_dispatch.is_none() {
                entry.after_dispatch = Some((now, order));
            }
            entry.snapshot = snapshot;
            entry.dirty = true;
        } else {
            if self.entries.len() >= self.capacity {
                return Err(DirtyError::Capacity);
            }
            self.entries.insert(
                snapshot.object_id,
                Entry {
                    snapshot,
                    in_flight: None,
                    dirty: true,
                    reserved: false,
                    dirty_since: Some(now),
                    dirty_order: order,
                    after_dispatch: None,
                },
            );
        }
        Ok(())
    }
    /// Admit one immutable capture atomically, retaining first-dirty ordering.
    pub fn mark_batch_at(
        &mut self,
        mut snapshots: Vec<SaveSnapshot>,
        now: Duration,
    ) -> Result<(), DirtyError> {
        snapshots.retain(|s| {
            !self
                .entries
                .get(&s.object_id)
                .is_some_and(|e| e.snapshot == *s)
        });
        let mut ids = std::collections::BTreeSet::new();
        let mut bytes = self.retained_bytes();
        let mut fresh = 0usize;
        if self
            .next_order
            .checked_add(snapshots.len() as u64)
            .is_none()
        {
            return Err(DirtyError::Stale);
        }
        for s in &snapshots {
            if !ids.insert(s.object_id)
                || s.expected_version < 0
                || s.bytes.len() > 17 * 1024 * 1024
            {
                return Err(DirtyError::Stale);
            }
            if let Some(e) = self.entries.get(&s.object_id) {
                if e.reserved {
                    return Err(DirtyError::Busy);
                }
                if s.mutation_revision <= e.snapshot.mutation_revision
                    || s.expected_version != e.snapshot.expected_version
                {
                    return Err(DirtyError::Stale);
                }
                bytes = bytes.saturating_sub(e.snapshot.bytes.len());
            } else {
                fresh += 1;
            }
            bytes = bytes
                .checked_add(s.bytes.len())
                .ok_or(DirtyError::Capacity)?;
            if bytes > self.byte_limit {
                return Err(DirtyError::Capacity);
            }
        }
        if bytes > self.byte_limit || self.entries.len() + fresh > self.capacity {
            return Err(DirtyError::Capacity);
        }
        for s in snapshots {
            self.mark_at(s, now)?;
        }
        Ok(())
    }
    pub fn due(&mut self, now: Duration) -> Vec<SaveSnapshot> {
        self.last_now = self.last_now.max(now);
        self.select_ready(Some(now))
    }
    /// Oldest-deadline first, with admission-order ties. Shutdown ignores deadlines.
    pub fn drain_ready(&mut self) -> Vec<SaveSnapshot> {
        self.select_ready(None)
    }
    fn select_ready(&mut self, now: Option<Duration>) -> Vec<SaveSnapshot> {
        let mut ready: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(&id, entry)| {
                let since = entry.dirty_since?;
                if !entry.dirty
                    || entry.in_flight.is_some()
                    || entry.reserved
                    || now.is_some_and(|now| now < since.saturating_add(SAVE_INTERVAL))
                {
                    return None;
                }
                Some((since, entry.dirty_order, id))
            })
            .collect();
        ready.sort_unstable();
        let mut bytes = self.in_flight_bytes();
        let mut batch = Vec::new();
        for (_, _, id) in ready {
            let entry = self.entries.get_mut(&id).expect("selected existing entry");
            // Do not backfill with small/new requests while the oldest waits for byte capacity.
            if batch.len() == 1024
                || bytes.saturating_add(entry.snapshot.bytes.len()) > self.byte_limit
            {
                break;
            }
            let snapshot = entry.snapshot.clone();
            bytes += snapshot.bytes.len();
            entry.in_flight = Some(InFlight {
                mutation_revision: snapshot.mutation_revision,
                expected_version: snapshot.expected_version,
                bytes: snapshot.bytes.len(),
            });
            entry.after_dispatch = None;
            batch.push(snapshot);
        }
        batch
    }
    pub fn dirty_since(&self, object_id: u32) -> Option<Duration> {
        self.entries.get(&object_id).and_then(|e| e.dirty_since)
    }
    pub fn acknowledge(&mut self, ack: &SaveAck) -> Result<(), DirtyError> {
        let entry = self
            .entries
            .get_mut(&ack.object_id)
            .ok_or(DirtyError::Unknown)?;
        let sent = entry.in_flight.as_ref().ok_or(DirtyError::Stale)?;
        if sent.mutation_revision != ack.mutation_revision
            || sent.expected_version.checked_add(1) != Some(ack.persisted_version)
        {
            return Err(DirtyError::Stale);
        }
        entry.snapshot.expected_version = ack.persisted_version;
        entry.dirty = entry.snapshot.mutation_revision != ack.mutation_revision;
        if entry.dirty {
            let (since, order) = entry.after_dispatch.take().ok_or(DirtyError::Stale)?;
            entry.dirty_since = Some(since);
            entry.dirty_order = order;
        } else {
            entry.dirty_since = None;
            entry.after_dispatch = None;
        }
        entry.in_flight = None;
        Ok(())
    }
    /// A confirmed rollback/failure releases work for retry; ambiguous commits need resolution first.
    pub fn failed(&mut self, request: &SaveSnapshot) -> Result<(), DirtyError> {
        let entry = self
            .entries
            .get_mut(&request.object_id)
            .ok_or(DirtyError::Unknown)?;
        if entry.in_flight.as_ref().is_none_or(|sent| {
            sent.mutation_revision != request.mutation_revision
                || sent.expected_version != request.expected_version
        }) {
            return Err(DirtyError::Stale);
        }
        entry.in_flight = None;
        Ok(())
    }
    /// Atomically reserve all participants after preceding routine writes complete.
    /// Conflicting mutation remains blocked until finish_reserved or cancel_reserved.
    pub fn reserve(&mut self, ids: &[u32]) -> Result<Vec<SaveSnapshot>, DirtyError> {
        let unique: std::collections::BTreeSet<_> = ids.iter().copied().collect();
        if unique.len() != ids.len() || ids.is_empty() {
            return Err(DirtyError::Stale);
        }
        for id in ids {
            let entry = self.entries.get(id).ok_or(DirtyError::Unknown)?;
            if entry.in_flight.is_some() || entry.reserved {
                return Err(DirtyError::Busy);
            }
        }
        let mut snapshots = Vec::with_capacity(ids.len());
        for id in ids {
            let entry = self.entries.get_mut(id).ok_or(DirtyError::Unknown)?;
            entry.reserved = true;
            snapshots.push(entry.snapshot.clone());
        }
        Ok(snapshots)
    }
    /// Call only after the valuable transaction is confirmed committed. Each snapshot carries
    /// the committed database version, rather than the transaction's expected old version.
    pub fn finish_reserved(&mut self, committed: &[SaveSnapshot]) -> Result<(), DirtyError> {
        let unique: std::collections::BTreeSet<_> = committed.iter().map(|s| s.object_id).collect();
        if unique.len() != committed.len() || committed.is_empty() {
            return Err(DirtyError::Stale);
        }
        let mut projected = self.retained_bytes();
        for snapshot in committed {
            let entry = self
                .entries
                .get(&snapshot.object_id)
                .ok_or(DirtyError::Unknown)?;
            if snapshot.bytes.len() > 17 * 1024 * 1024 {
                return Err(DirtyError::Capacity);
            }
            projected = projected
                .saturating_sub(entry.snapshot.bytes.len())
                .saturating_add(snapshot.bytes.len());
            if !entry.reserved
                || snapshot.mutation_revision <= entry.snapshot.mutation_revision
                || entry.snapshot.expected_version.checked_add(1) != Some(snapshot.expected_version)
            {
                return Err(DirtyError::Stale);
            }
        }
        if projected > self.byte_limit {
            return Err(DirtyError::Capacity);
        }
        for snapshot in committed {
            let entry = self
                .entries
                .get_mut(&snapshot.object_id)
                .ok_or(DirtyError::Unknown)?;
            entry.snapshot = snapshot.clone();
            entry.dirty = false;
            entry.dirty_since = None;
            entry.after_dispatch = None;
            entry.reserved = false;
        }
        Ok(())
    }
    /// Confirm a hierarchy transaction, including newly acquired aggregates and
    /// retired ownership. All checks precede mutation; unchanged reserved entries
    /// retain their dirty revisions and original age.
    pub fn finish_reserved_hierarchy(
        &mut self,
        reserved: &[u32],
        changed: &[SaveSnapshot],
        acquired: &[SaveSnapshot],
        retired: &[u32],
    ) -> Result<(), DirtyError> {
        let reserved_ids: std::collections::BTreeSet<_> = reserved.iter().copied().collect();
        let retired_ids: std::collections::BTreeSet<_> = retired.iter().copied().collect();
        if reserved_ids.len() != reserved.len()
            || retired_ids.len() != retired.len()
            || !retired_ids.is_subset(&reserved_ids)
        {
            return Err(DirtyError::Stale);
        }
        for id in reserved {
            if !self
                .entries
                .get(id)
                .is_some_and(|e| e.reserved && e.in_flight.is_none())
            {
                return Err(DirtyError::Busy);
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut bytes = self.retained_bytes();
        for id in retired {
            bytes = bytes.saturating_sub(self.entries[id].snapshot.bytes.len());
        }
        for s in changed {
            if !reserved_ids.contains(&s.object_id) || !seen.insert(s.object_id) {
                return Err(DirtyError::Stale);
            }
            let e = &self.entries[&s.object_id];
            if s.mutation_revision <= e.snapshot.mutation_revision
                || e.snapshot.expected_version.checked_add(1) != Some(s.expected_version)
                || s.bytes.len() > 17 * 1024 * 1024
            {
                return Err(DirtyError::Stale);
            }
            if !retired_ids.contains(&s.object_id) {
                bytes = bytes
                    .saturating_sub(e.snapshot.bytes.len())
                    .checked_add(s.bytes.len())
                    .ok_or(DirtyError::Capacity)?;
            }
        }
        for s in acquired {
            if self.entries.contains_key(&s.object_id)
                || !seen.insert(s.object_id)
                || s.expected_version <= 0
                || s.bytes.len() > 17 * 1024 * 1024
            {
                return Err(DirtyError::Stale);
            }
            bytes = bytes
                .checked_add(s.bytes.len())
                .ok_or(DirtyError::Capacity)?;
        }
        if bytes > self.byte_limit
            || self.entries.len() - retired.len() + acquired.len() > self.capacity
        {
            return Err(DirtyError::Capacity);
        }
        for id in reserved {
            self.entries
                .get_mut(id)
                .expect("validated reservation")
                .reserved = false;
        }
        for s in changed
            .iter()
            .filter(|s| !retired_ids.contains(&s.object_id))
        {
            let e = self
                .entries
                .get_mut(&s.object_id)
                .expect("validated changed row");
            e.snapshot = s.clone();
            e.dirty = false;
            e.dirty_since = None;
            e.after_dispatch = None;
        }
        for id in retired {
            self.entries.remove(id);
        }
        for s in acquired {
            self.entries.insert(
                s.object_id,
                Entry {
                    snapshot: s.clone(),
                    in_flight: None,
                    dirty: false,
                    reserved: false,
                    dirty_since: None,
                    dirty_order: 0,
                    after_dispatch: None,
                },
            );
        }
        Ok(())
    }
    /// Confirm rollback before cancellation; uncertain commits must be resolved first.
    pub fn cancel_reserved(&mut self, ids: &[u32]) -> Result<(), DirtyError> {
        for id in ids {
            if !self.entries.get(id).ok_or(DirtyError::Unknown)?.reserved {
                return Err(DirtyError::Stale);
            }
        }
        for id in ids {
            self.entries
                .get_mut(id)
                .ok_or(DirtyError::Unknown)?
                .reserved = false;
        }
        Ok(())
    }
    pub fn retained_bytes(&self) -> usize {
        self.entries.values().map(|e| e.snapshot.bytes.len()).sum()
    }
    pub fn in_flight_bytes(&self) -> usize {
        self.entries
            .values()
            .filter_map(|e| e.in_flight.as_ref())
            .map(|s| s.bytes)
            .sum()
    }
    pub fn is_clean(&self) -> bool {
        self.entries
            .values()
            .all(|e| !e.dirty && e.in_flight.is_none() && !e.reserved)
    }
    pub fn is_object_clean(&self, object_id: u32) -> bool {
        self.entries
            .get(&object_id)
            .is_some_and(|e| !e.dirty && e.in_flight.is_none() && !e.reserved)
    }
    pub fn forget_clean_batch(&mut self, ids: &[u32]) -> Result<(), DirtyError> {
        let unique: std::collections::BTreeSet<_> = ids.iter().copied().collect();
        if ids.is_empty() || unique.len() != ids.len() {
            return Err(DirtyError::Stale);
        }
        for id in ids {
            let e = self.entries.get(id).ok_or(DirtyError::Unknown)?;
            if e.dirty || e.in_flight.is_some() || e.reserved {
                return Err(DirtyError::Busy);
            }
        }
        for id in ids {
            self.entries.remove(id);
        }
        Ok(())
    }
    pub fn forget_clean(&mut self, object_id: u32) -> Result<(), DirtyError> {
        let entry = self.entries.get(&object_id).ok_or(DirtyError::Unknown)?;
        if entry.dirty || entry.in_flight.is_some() || entry.reserved {
            return Err(DirtyError::Busy);
        }
        self.entries.remove(&object_id);
        Ok(())
    }
}
