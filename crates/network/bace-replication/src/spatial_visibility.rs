//! Pinned ACE ObjectMaint initial clamp/25-second knowledge retention. A staged
//! delta changes knowledge only after its exact reliable output is admitted.
use crate::{ForgetTicket, ReplicationError, Visibility};
use bace_gameplay_api::{CharacterBinding, visibility::VisibilitySnapshot};
use bace_types::EntityId;
const FORGET_MS: u64 = 25_000;
const INITIAL_DISTANCE_SQUARED: f32 = 112.5 * 112.5;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialVisibilityError {
    Busy,
    WrongBinding,
    StaleSnapshot,
    InvalidSnapshot,
    InvalidTicket,
    Knowledge(ReplicationError),
}
impl From<ReplicationError> for SpatialVisibilityError {
    fn from(v: ReplicationError) -> Self {
        Self::Knowledge(v)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibilityDelta {
    pub ticket: u64,
    pub binding: CharacterBinding,
    pub observer_epoch: u16,
    pub tick: u64,
    /// Encode removals before creates, including expired objects re-entering PVS.
    pub removes: Vec<EntityId>,
    pub creates: Vec<EntityId>,
}
enum Pending {
    Snapshot(VisibilitySnapshot),
    Retirement {
        entity: EntityId,
        tick: u64,
    },
    Reset {
        tick: u64,
    },
    Projectile {
        entity: EntityId,
        tick: u64,
        epoch: u16,
    },
}
pub struct SpatialVisibility {
    binding: CharacterBinding,
    knowledge: Visibility,
    capacity: usize,
    generation: u64,
    last_tick: Option<u64>,
    last_epoch: u16,
    pending: Option<Pending>,
    delta: VisibilityDelta,
    selected: Vec<EntityId>,
    absent: Vec<EntityId>,
    expired: Vec<ForgetTicket>,
}
fn milliseconds(tick: u64) -> Result<u64, SpatialVisibilityError> {
    tick.checked_mul(1000)
        .map(|v| v / 30)
        .ok_or(SpatialVisibilityError::InvalidSnapshot)
}
impl SpatialVisibility {
    pub fn new(binding: CharacterBinding, capacity: usize) -> Result<Self, SpatialVisibilityError> {
        if binding.actor.0 == 0 || binding.account.0 == 0 || binding.session.0 == 0 {
            return Err(SpatialVisibilityError::WrongBinding);
        }
        Ok(Self {
            binding,
            knowledge: Visibility::new(capacity)?,
            capacity,
            generation: 0,
            last_tick: None,
            last_epoch: 0,
            pending: None,
            delta: VisibilityDelta {
                ticket: 0,
                binding,
                observer_epoch: 0,
                tick: 0,
                removes: Vec::with_capacity(capacity),
                creates: Vec::with_capacity(capacity),
            },
            selected: Vec::with_capacity(capacity),
            absent: Vec::with_capacity(capacity),
            expired: Vec::with_capacity(capacity),
        })
    }
    pub fn knows(&self, entity: EntityId) -> bool {
        self.knowledge.knows(entity)
    }
    pub fn visible(&self, entity: EntityId) -> bool {
        self.knowledge.visible(entity)
    }
    pub fn known_entities(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.knowledge.known_entities()
    }
    pub fn pending(&self) -> Option<&VisibilityDelta> {
        self.pending.as_ref().map(|_| &self.delta)
    }
    pub fn stage(
        &mut self,
        snapshot: VisibilitySnapshot,
    ) -> Result<&VisibilityDelta, (SpatialVisibilityError, VisibilitySnapshot)> {
        if let Err(error) = self.prepare(&snapshot) {
            return Err((error, snapshot));
        }
        self.pending = Some(Pending::Snapshot(snapshot));
        Ok(&self.delta)
    }
    fn prepare(&mut self, snapshot: &VisibilitySnapshot) -> Result<(), SpatialVisibilityError> {
        if self.pending.is_some() {
            return Err(SpatialVisibilityError::Busy);
        }
        if snapshot.binding != self.binding {
            return Err(SpatialVisibilityError::WrongBinding);
        }
        if self.last_tick.is_some_and(|old| snapshot.tick < old)
            || (self.last_tick.is_some()
                && snapshot.observer_epoch != self.last_epoch
                && snapshot.observer_epoch.wrapping_sub(self.last_epoch) >= 0x8000)
        {
            return Err(SpatialVisibilityError::StaleSnapshot);
        }
        if snapshot.candidates.len() > 65536
            || snapshot
                .candidates
                .windows(2)
                .any(|p| p[0].entity >= p[1].entity)
            || snapshot.candidates.iter().any(|c| {
                c.entity.0 == 0
                    || c.entity == self.binding.actor
                    || !c.distance_squared.is_finite()
                    || c.distance_squared < 0.0
            })
        {
            return Err(SpatialVisibilityError::InvalidSnapshot);
        }
        let now = milliseconds(snapshot.tick)?;
        now.checked_add(FORGET_MS)
            .ok_or(SpatialVisibilityError::InvalidSnapshot)?;
        let ticket = self
            .generation
            .checked_add(1)
            .ok_or(ReplicationError::GenerationExhausted)?;
        self.selected.clear();
        self.absent.clear();
        self.expired.clear();
        self.delta.creates.clear();
        self.delta.removes.clear();
        self.expired.extend(
            self.knowledge
                .pending_forgets()
                .filter(|t| t.deadline_ms() <= now),
        );
        self.delta
            .removes
            .extend(self.expired.iter().map(|t| t.entity()));
        for candidate in &snapshot.candidates {
            let expired = self.delta.removes.binary_search(&candidate.entity).is_ok();
            let known = self.knowledge.knows(candidate.entity) && !expired;
            if known || candidate.distance_squared <= INITIAL_DISTANCE_SQUARED {
                if self.selected.len() == self.capacity {
                    return Err(ReplicationError::Capacity.into());
                }
                self.selected.push(candidate.entity);
                if !known {
                    self.delta.creates.push(candidate.entity);
                }
            }
        }
        self.knowledge.preflight_changes(
            now,
            self.selected.len(),
            self.expired.len(),
            self.delta.creates.len(),
        )?;
        self.absent
            .extend(self.knowledge.known_entities().filter(|id| {
                self.selected.binary_search(id).is_err()
                    && self.delta.removes.binary_search(id).is_err()
            }));
        self.generation = ticket;
        self.delta.ticket = ticket;
        self.delta.observer_epoch = snapshot.observer_epoch;
        self.delta.tick = snapshot.tick;
        Ok(())
    }
    /// Actual authoritative destruction bypasses the PVS grace period. The same
    /// output receipt is still required before forgetting client knowledge.
    pub fn stage_retirement(
        &mut self,
        entity: EntityId,
        tick: u64,
    ) -> Result<&VisibilityDelta, SpatialVisibilityError> {
        if self.pending.is_some() {
            return Err(SpatialVisibilityError::Busy);
        }
        let tick = self.last_tick.map_or(tick, |old| old.max(tick));
        if entity.0 == 0 {
            return Err(SpatialVisibilityError::StaleSnapshot);
        }
        let now = milliseconds(tick)?;
        self.knowledge.preflight_changes(now, 0, 0, 0)?;
        let ticket = self
            .generation
            .checked_add(1)
            .ok_or(ReplicationError::GenerationExhausted)?;
        self.selected.clear();
        self.absent.clear();
        self.expired.clear();
        self.delta.creates.clear();
        self.delta.removes.clear();
        if self.knowledge.knows(entity) {
            self.delta.removes.push(entity);
        }
        self.generation = ticket;
        self.delta.ticket = ticket;
        self.delta.tick = tick;
        self.delta.observer_epoch = self.last_epoch;
        self.pending = Some(Pending::Retirement { entity, tick });
        Ok(&self.delta)
    }
    pub fn stage_reset(&mut self, tick: u64) -> Result<&VisibilityDelta, SpatialVisibilityError> {
        if self.pending.is_some() {
            return Err(SpatialVisibilityError::Busy);
        }
        let tick = self.last_tick.map_or(tick, |old| old.max(tick));
        self.knowledge
            .preflight_changes(milliseconds(tick)?, 0, 0, 0)?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(ReplicationError::GenerationExhausted)?;
        self.delta.ticket = self.generation;
        self.delta.tick = tick;
        self.delta.observer_epoch = self.last_epoch;
        self.delta.creates.clear();
        self.delta.removes.clear();
        self.delta.removes.extend(self.knowledge.known_entities());
        self.pending = Some(Pending::Reset { tick });
        Ok(&self.delta)
    }
    /// A reused entity ID is a new incarnation. Its old instance must be
    /// removed before the new create within the same receipt-fenced plan.
    pub fn recreate_pending(&mut self, entity: EntityId) -> Result<(), SpatialVisibilityError> {
        if !matches!(
            self.pending,
            Some(Pending::Snapshot(_) | Pending::Projectile { .. })
        ) || self.selected.binary_search(&entity).is_err()
        {
            return Err(SpatialVisibilityError::InvalidSnapshot);
        }
        if self.knowledge.knows(entity) && !self.delta.removes.contains(&entity) {
            self.delta.removes.push(entity);
            self.delta.removes.sort_unstable();
        }
        if !self.delta.creates.contains(&entity) {
            self.delta.creates.push(entity);
            self.delta.creates.sort_unstable();
        }
        Ok(())
    }
    /// Discard only a plan for which no reliable batch has been submitted.
    /// Consumes its ticket, preventing a delayed stale commit from matching.
    pub fn discard_unpublished(
        &mut self,
        ticket: u64,
    ) -> Result<Option<VisibilitySnapshot>, SpatialVisibilityError> {
        if self.pending.is_none() || self.delta.ticket != ticket {
            return Err(SpatialVisibilityError::InvalidTicket);
        }
        Ok(match self.pending.take().expect("checked pending") {
            Pending::Snapshot(snapshot) => Some(snapshot),
            Pending::Retirement { .. } | Pending::Reset { .. } | Pending::Projectile { .. } => None,
        })
    }
    /// Only call after the complete exact encoded delta is admitted by the
    /// reliable owner. On uncertain admission, retain pending and its ticket.
    /// Returns the snapshot's reusable candidate buffer to the querying adapter.
    pub fn commit(
        &mut self,
        ticket: u64,
    ) -> Result<Option<VisibilitySnapshot>, SpatialVisibilityError> {
        if self.pending.is_none() || self.delta.ticket != ticket {
            return Err(SpatialVisibilityError::InvalidTicket);
        }
        let now = milliseconds(self.delta.tick)?;
        match self.pending.take().expect("checked pending delta") {
            Pending::Snapshot(snapshot) => {
                for &expired in &self.expired {
                    self.knowledge
                        .commit_forget(expired, now)
                        .expect("prepared knowledge mutation");
                }
                for &id in &self.selected {
                    self.knowledge
                        .observe(id, now)
                        .expect("prepared observation capacity");
                }
                for &id in &self.absent {
                    self.knowledge
                        .forget_later(id, now, FORGET_MS)
                        .expect("prepared deadline");
                }
                self.last_tick = Some(snapshot.tick);
                self.last_epoch = snapshot.observer_epoch;
                Ok(Some(snapshot))
            }
            Pending::Projectile {
                entity,
                tick,
                epoch,
            } => {
                self.knowledge
                    .observe(entity, now)
                    .expect("prepared launch capacity");
                self.last_tick = Some(tick);
                self.last_epoch = epoch;
                Ok(None)
            }
            Pending::Reset { tick } => {
                for &entity in &self.delta.removes {
                    self.knowledge
                        .forget_now(entity, now)
                        .expect("prepared reset");
                }
                self.last_tick = Some(tick);
                Ok(None)
            }
            Pending::Retirement { entity, tick } => {
                self.knowledge
                    .forget_now(entity, now)
                    .expect("prepared retirement");
                self.last_tick = Some(tick);
                Ok(None)
            }
        }
    }
}

impl SpatialVisibility {
    /// A retained launch adds exactly one source-visible projectile. It does not
    /// reinterpret an old PVS snapshot as the observer's current whole scene.
    pub fn stage_projectile(
        &mut self,
        entity: EntityId,
        epoch: u16,
        tick: u64,
        distance_squared: f32,
    ) -> Result<Option<&VisibilityDelta>, SpatialVisibilityError> {
        if self.pending.is_some() {
            return Err(SpatialVisibilityError::Busy);
        }
        if entity.0 == 0 || !distance_squared.is_finite() || distance_squared < 0.0 {
            return Err(SpatialVisibilityError::InvalidSnapshot);
        }
        if self.last_tick.is_some()
            && epoch != self.last_epoch
            && epoch.wrapping_sub(self.last_epoch) >= 0x8000
        {
            return Err(SpatialVisibilityError::StaleSnapshot);
        }
        let known = self.knowledge.knows(entity);
        if !known && distance_squared > INITIAL_DISTANCE_SQUARED {
            return Ok(None);
        }
        let tick = self.last_tick.map_or(tick, |old| old.max(tick));
        let now = milliseconds(tick)?;
        self.knowledge
            .preflight_changes(now, 1, 0, usize::from(!known))?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(ReplicationError::GenerationExhausted)?;
        self.selected.clear();
        self.selected.push(entity);
        self.absent.clear();
        self.expired.clear();
        self.delta.creates.clear();
        self.delta.removes.clear();
        if !known {
            self.delta.creates.push(entity);
        }
        self.delta.ticket = self.generation;
        self.delta.tick = tick;
        self.delta.observer_epoch = epoch;
        self.pending = Some(Pending::Projectile {
            entity,
            tick,
            epoch,
        });
        Ok(Some(&self.delta))
    }
}
