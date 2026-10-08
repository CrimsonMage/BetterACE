//! Bounded PVS query / accepted view / reliable-receipt pipeline. Descriptions
//! are immutable cold-prepared rendering inputs, never a second world owner.
use crate::{
    network::{NetworkCommand, NetworkThread},
    player_service::PlayerService,
    simulation::{SimulationInput, SimulationWorker},
};
use bace_gameplay_api::{CharacterBinding, visibility::*};
use bace_replication::{ObjectProjection, ReplicationMessage, Sequences, SpatialVisibility};
use bace_session::SessionKey;
use bace_simulation::Command;
use bace_types::EntityId;
use bace_wire::{ObjectCodecLimits, ObjectControl, ObjectDescription};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, mpsc::TrySendError},
};
mod ammunition;
mod delivery;
mod equipment;
mod observer_fence;
mod projectiles;
mod projection;
#[derive(Clone, Copy, Debug)]
pub struct VisibilityLimits {
    pub observers: usize,
    pub objects: usize,
    pub known_per_observer: usize,
    pub batch_bytes: usize,
    pub retained_bytes: usize,
    pub codec: ObjectCodecLimits,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisibilityServiceError {
    Busy,
    Capacity,
    InvalidDescription,
    Projection,
    Stale,
    WorkerClosed,
    NetworkClosed,
    Rejected,
}
#[derive(Clone)]
struct Blueprint {
    incarnation: u64,
    revision: u64,
    admitted_tick: u64,
    description: Arc<ObjectDescription>,
    children: Vec<Arc<ObjectDescription>>,
}
struct Object {
    ammunition_receipt: Option<Box<ammunition::AmmunitionReceipt>>,
    initial_character_revision: u64,
    equipment_receipt: Option<equipment::EquipmentReceipt>,
    blueprint: Arc<Blueprint>,
    sequences: Sequences,
    projection: Option<ObjectProjection>,
    tick: u64,
    version: u64,
    projection_revision: u64,
}
#[derive(Clone)]
struct Sent {
    blueprint: Arc<Blueprint>,
    version: u64,
}
struct Publication {
    self_sent: Option<Sent>,
    ticket: u64,
    messages: VecDeque<Vec<(u16, Vec<u8>)>>,
    inflight: Option<u64>,
    sent: BTreeMap<EntityId, Sent>,
    removes: Vec<EntityId>,
    bytes: usize,
}
struct Observer {
    launches: std::collections::BTreeSet<EntityId>,
    self_sent: Option<Sent>,
    force_self: bool,
    binding: CharacterBinding,
    knowledge: SpatialVisibility,
    buffer: Vec<VisibilityCandidate>,
    query: Option<u64>,
    view_query: Option<u64>,
    view_request: Option<ObjectViewRequest>,
    publication: Option<Publication>,
    sent: BTreeMap<EntityId, Sent>,
    retirements: VecDeque<(EntityId, u64)>,
    failed: Option<VisibilityServiceError>,
    reset_requested: Option<u64>,
    reset_started: Option<u64>,
    reset_event: Option<(u64, u64)>,
}
pub struct VisibilityService {
    launches: BTreeMap<EntityId, projectiles::RetainedLaunch>,
    limits: VisibilityLimits,
    objects: BTreeMap<EntityId, Object>,
    observers: BTreeMap<SessionKey, Observer>,
    next: u64,
    retained_bytes: usize,
    source_allocations: Vec<(std::sync::Weak<Blueprint>, usize)>,
    cursor: Option<SessionKey>,
}
impl VisibilityService {
    /// Read-only authored display name already accepted by the visible object
    /// owner. Staff command preparation copies this before simulation admission.
    pub fn registered_object_name(&self, entity: EntityId) -> Option<&str> {
        self.objects
            .get(&entity)
            .map(|object| object.blueprint.description.game.name.as_str())
    }

    pub fn new(limits: VisibilityLimits) -> Result<Self, VisibilityServiceError> {
        if !(1..=4096).contains(&limits.observers)
            || !(1..=65536).contains(&limits.objects)
            || !(1..=4096).contains(&limits.known_per_observer)
            || limits.batch_bytes < 64
            || limits.retained_bytes < limits.batch_bytes
            || limits.codec.max_message_bytes > limits.batch_bytes
        {
            return Err(VisibilityServiceError::Capacity);
        }
        Ok(Self {
            launches: BTreeMap::new(),
            limits,
            objects: BTreeMap::new(),
            observers: BTreeMap::new(),
            next: 0x5600_0000_0000_0000,
            retained_bytes: 0,
            source_allocations: Vec::new(),
            cursor: None,
        })
    }
    pub fn register_object(
        &mut self,
        incarnation: u64,
        revision: u64,
        admitted_tick: u64,
        description: Arc<ObjectDescription>,
        children: Vec<Arc<ObjectDescription>>,
    ) -> Result<(), VisibilityServiceError> {
        let id = EntityId(description.object_id);
        if let Some(old) = self.objects.get(&id).filter(|old| {
            old.blueprint.incarnation == incarnation && old.blueprint.revision == revision
        }) {
            return if old.blueprint.description == description && old.blueprint.children == children
            {
                Ok(())
            } else {
                Err(VisibilityServiceError::Stale)
            };
        }
        if id.0 == 0
            || incarnation == 0
            || revision == 0
            || children.len() > self.limits.codec.max_children
            || description.physics.options.parent.is_some()
            || description.physics.options.children.len() != children.len()
        {
            return Err(VisibilityServiceError::InvalidDescription);
        }
        let mut source_bytes = 0usize;
        for child in &children {
            if child
                .physics
                .options
                .parent
                .is_none_or(|p| p.object_id != id.0)
                || !description
                    .physics
                    .options
                    .children
                    .iter()
                    .any(|p| p.object_id == child.object_id)
                || children
                    .iter()
                    .filter(|c| c.object_id == child.object_id)
                    .count()
                    != 1
            {
                return Err(VisibilityServiceError::InvalidDescription);
            }
            source_bytes += child
                .encode_create(self.limits.codec)
                .map_err(|_| VisibilityServiceError::InvalidDescription)?
                .len();
        }
        source_bytes += description
            .encode_create(self.limits.codec)
            .map_err(|_| VisibilityServiceError::InvalidDescription)?
            .len();
        self.source_allocations
            .retain(|(source, _)| source.strong_count() != 0);
        if self.source_allocations.len() == 65536
            || self
                .source_allocations
                .iter()
                .map(|(_, size)| size)
                .sum::<usize>()
                .saturating_add(source_bytes)
                > 64 * 1024 * 1024
        {
            return Err(VisibilityServiceError::Capacity);
        }
        if let Some(old) = self.objects.get(&id) {
            if incarnation < old.blueprint.incarnation
                || incarnation == old.blueprint.incarnation && revision < old.blueprint.revision
            {
                return Err(VisibilityServiceError::Stale);
            }
        } else if self.objects.len() == self.limits.objects {
            return Err(VisibilityServiceError::Capacity);
        }
        let blueprint = Arc::new(Blueprint {
            incarnation,
            revision,
            admitted_tick,
            description,
            children,
        });
        self.source_allocations
            .push((Arc::downgrade(&blueprint), source_bytes));
        if let Some(old) = self
            .objects
            .get_mut(&id)
            .filter(|old| old.blueprint.incarnation == incarnation)
        {
            old.blueprint = blueprint;
            return Ok(());
        }
        let sequences =
            Sequences::with_instance(10, blueprint.description.physics.sequences.instance)
                .map_err(|_| VisibilityServiceError::Capacity)?;
        self.objects.insert(
            id,
            Object {
                ammunition_receipt: None,
                initial_character_revision: revision.saturating_sub(1),
                equipment_receipt: None,
                blueprint,
                sequences,
                projection: None,
                tick: 0,
                version: 0,
                projection_revision: 0,
            },
        );
        Ok(())
    }
    pub fn retire_object(
        &mut self,
        entity: EntityId,
        tick: u64,
    ) -> Result<(), VisibilityServiceError> {
        if self.launches.contains_key(&entity) {
            return Err(VisibilityServiceError::Busy);
        }
        for observer in self.observers.values() {
            if observer.retirements.len() == self.limits.objects
                && !observer.retirements.iter().any(|(id, _)| *id == entity)
            {
                return Err(VisibilityServiceError::Capacity);
            }
        }
        self.objects.remove(&entity);
        for observer in self.observers.values_mut() {
            if !observer.retirements.iter().any(|(id, _)| *id == entity) {
                observer.retirements.push_back((entity, tick));
            }
        }
        Ok(())
    }
    pub fn bind(
        &mut self,
        key: SessionKey,
        binding: CharacterBinding,
    ) -> Result<(), VisibilityServiceError> {
        if let Some(old) = self.observers.get(&key) {
            return if old.binding == binding {
                Ok(())
            } else {
                Err(VisibilityServiceError::Stale)
            };
        }
        if self.observers.len() == self.limits.observers {
            return Err(VisibilityServiceError::Capacity);
        }
        let knowledge = SpatialVisibility::new(binding, self.limits.known_per_observer)
            .map_err(|_| VisibilityServiceError::Capacity)?;
        self.observers.insert(
            key,
            Observer {
                launches: Default::default(),
                self_sent: None,
                force_self: false,
                binding,
                knowledge,
                buffer: Vec::new(),
                query: None,
                view_query: None,
                view_request: None,
                publication: None,
                sent: BTreeMap::new(),
                retirements: VecDeque::new(),
                failed: None,
                reset_requested: None,
                reset_started: None,
                reset_event: None,
            },
        );
        Ok(())
    }
    pub fn request_self_correction(
        &mut self,
        key: SessionKey,
    ) -> Result<(), VisibilityServiceError> {
        self.observers
            .get_mut(&key)
            .ok_or(VisibilityServiceError::Stale)?
            .force_self = true;
        Ok(())
    }
    /// Idempotence is tied to the accepted event, independently of simulation time.
    /// Two teleports in one tick still require two ordered visibility resets.
    pub fn reset_observer_event(
        &mut self,
        key: SessionKey,
        tick: u64,
        event: u64,
    ) -> Result<(), VisibilityServiceError> {
        let observer = self
            .observers
            .get_mut(&key)
            .ok_or(VisibilityServiceError::Stale)?;
        if event == 0
            || observer
                .reset_event
                .is_some_and(|(old, at)| old > event || (old == event && at != tick))
        {
            return Err(VisibilityServiceError::Stale);
        }
        if observer.reset_event.is_none_or(|(old, _)| old != event) {
            if observer.reset_requested.is_some() {
                return Err(VisibilityServiceError::Busy);
            }
            observer.reset_event = Some((event, tick));
            observer.reset_started = None;
        }
        self.reset_observer(key, tick)
    }
    pub fn reset_observer(
        &mut self,
        key: SessionKey,
        tick: u64,
    ) -> Result<(), VisibilityServiceError> {
        let o = self
            .observers
            .get_mut(&key)
            .ok_or(VisibilityServiceError::Stale)?;
        if o.reset_started.is_some_and(|old| old >= tick) {
            return Ok(());
        }
        o.reset_requested = Some(o.reset_requested.map_or(tick, |old| old.max(tick)));
        if o.publication.is_some()
            || o.query.is_some()
            || o.view_query.is_some()
            || o.view_request.is_some()
        {
            return Err(VisibilityServiceError::Busy);
        }
        let delta = o
            .knowledge
            .stage_reset(tick)
            .map_err(|_| VisibilityServiceError::Stale)?
            .clone();
        let mut messages = Vec::new();
        for id in &delta.removes {
            if let Some(sent) = o.sent.get(id) {
                delivery::append_deletes(&mut messages, &sent.blueprint);
            }
        }
        match self.publish(key, delta.ticket, messages, BTreeMap::new(), delta.removes) {
            Err(VisibilityServiceError::Capacity) => {
                self.observers
                    .get_mut(&key)
                    .expect("present")
                    .knowledge
                    .discard_unpublished(delta.ticket)
                    .map_err(|_| VisibilityServiceError::Projection)?;
                Err(VisibilityServiceError::Busy)
            }
            Ok(()) => {
                let o = self.observers.get_mut(&key).expect("present");
                o.reset_requested = None;
                o.reset_started = Some(tick);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
    pub fn unbind(&mut self, key: SessionKey) {
        if let Some(old) = self.observers.remove(&key)
            && let Some(p) = old.publication
        {
            self.retained_bytes -= p.bytes;
        }
    }
    pub fn observing_sessions(&self, entity: EntityId) -> impl Iterator<Item = SessionKey> + '_ {
        self.observers
            .iter()
            .filter(move |(_, o)| {
                o.failed.is_none()
                    && o.knowledge.knows(entity)
                    && o.knowledge
                        .pending()
                        .is_none_or(|p| !p.removes.contains(&entity))
            })
            .map(|(key, _)| *key)
    }
    pub fn failed_sessions(
        &self,
    ) -> impl Iterator<Item = (SessionKey, &VisibilityServiceError)> + '_ {
        self.observers
            .iter()
            .filter_map(|(key, o)| o.failed.as_ref().map(|e| (*key, e)))
    }
    pub fn failure(&self, key: SessionKey) -> Option<&VisibilityServiceError> {
        self.observers.get(&key).and_then(|o| o.failed.as_ref())
    }
    pub fn knows(&self, key: SessionKey, id: EntityId) -> bool {
        self.observers
            .get(&key)
            .is_some_and(|o| o.knowledge.knows(id))
    }
    pub fn pending(&self) -> bool {
        !self.launches.is_empty()
            || self.observers.values().any(|o| {
                o.query.is_some()
                    || o.view_query.is_some()
                    || o.publication.is_some()
                    || o.view_request.is_some()
                    || o.reset_requested.is_some()
                    || !o.retirements.is_empty()
            })
    }
    fn next(&mut self) -> Result<u64, VisibilityServiceError> {
        self.next = self
            .next
            .checked_add(1)
            .ok_or(VisibilityServiceError::Capacity)?;
        Ok(self.next)
    }
    pub fn poll(
        &mut self,
        worker: &SimulationWorker,
        players: &mut PlayerService,
        network: &NetworkThread,
    ) -> Result<(), VisibilityServiceError> {
        let bindings: Vec<_> = players.entered_bindings().collect();
        let gone: Vec<_> = self
            .observers
            .keys()
            .filter(|key| !bindings.iter().any(|(k, _)| k == *key))
            .copied()
            .collect();
        for key in gone {
            self.unbind(key);
        }
        for (key, binding) in bindings {
            self.bind(key, binding)?;
        }
        while let Ok(outcome) = worker.visibility_outcomes().try_recv() {
            let outcome =
                Arc::try_unwrap(outcome).map_err(|_| VisibilityServiceError::Projection)?;
            self.accept_visibility(outcome)?;
        }
        while let Ok(outcome) = worker.object_view_outcomes().try_recv() {
            self.accept_views(&outcome, players)?;
        }
        self.drive(&worker.input(), network)
    }
}

#[cfg(test)]
impl VisibilityService {
    /// Supply a synthetic accepted owner result to the normal query/view
    /// projectors. Portal tests use this only after registering an immutable
    /// blueprint; neither knowledge nor publication is advanced here.
    pub(crate) fn test_stage_accepted_nonplayer(
        &mut self,
        key: SessionKey,
        tick: u64,
        view: AcceptedObjectView,
    ) -> Result<(), VisibilityServiceError> {
        let observer = self
            .observers
            .get(&key)
            .ok_or(VisibilityServiceError::Stale)?;
        if observer.query.is_some()
            || observer.view_query.is_some()
            || observer.publication.is_some()
            || observer.reset_requested.is_some()
        {
            return Err(VisibilityServiceError::Busy);
        }
        let binding = observer.binding;
        let correlation = self.next()?;
        self.observers
            .get_mut(&key)
            .ok_or(VisibilityServiceError::Stale)?
            .query = Some(correlation);
        self.accept_visibility(VisibilityOutcome {
            correlation,
            result: Ok(VisibilitySnapshot {
                binding,
                observer_epoch: 0,
                tick,
                candidates: vec![VisibilityCandidate {
                    entity: view.entity,
                    distance_squared: 1.0,
                }],
            }),
        })?;
        let request = self
            .observers
            .get_mut(&key)
            .ok_or(VisibilityServiceError::Stale)?
            .view_request
            .take()
            .ok_or(VisibilityServiceError::Projection)?;
        if request.entities != [view.entity] {
            return Err(VisibilityServiceError::Projection);
        }
        self.observers
            .get_mut(&key)
            .ok_or(VisibilityServiceError::Stale)?
            .view_query = Some(request.correlation);
        self.accept_views_inner(
            &ObjectViewOutcome {
                correlation: request.correlation,
                result: Ok(ObjectViewSnapshot {
                    binding,
                    observer_epoch: 0,
                    tick,
                    views: vec![(view.entity, Ok(view))],
                }),
            },
            None,
        )
    }

    /// Mirror a reliable queue admission, returning its exact receipt key and
    /// bytes. The caller must acknowledge via `reliable_admission`.
    pub(crate) fn test_submit_reliable(
        &mut self,
        key: SessionKey,
    ) -> Result<(u64, Vec<Vec<u8>>), VisibilityServiceError> {
        let observer = self
            .observers
            .get(&key)
            .ok_or(VisibilityServiceError::Stale)?;
        let publication = observer
            .publication
            .as_ref()
            .ok_or(VisibilityServiceError::Stale)?;
        if publication.inflight.is_some() {
            return Err(VisibilityServiceError::Busy);
        }
        let correlation = self.next()?;
        let publication = self
            .observers
            .get_mut(&key)
            .ok_or(VisibilityServiceError::Stale)?
            .publication
            .as_mut()
            .ok_or(VisibilityServiceError::Stale)?;
        let bytes = publication
            .messages
            .front()
            .ok_or(VisibilityServiceError::Projection)?
            .iter()
            .map(|(_, bytes)| bytes.clone())
            .collect();
        publication.inflight = Some(correlation);
        Ok((correlation, bytes))
    }
}

#[cfg(test)]
mod tests;
