//! Bounded production generator materialization and durable lifecycle delivery.
//! The simulation remains the sole entity/placement/generator owner. This service
//! retains immutable cold results, allocated identities and exact output fences.
mod cold;
mod construction;
mod contained;
mod delivery;
mod materialization;
mod publications;
pub use publications::PreparedGeneratorPublication;
mod mixed;
mod regions;
mod repository;
pub use regions::GeneratorRegions;
mod retirement;
mod stock;
use crate::{
    region_activation::PreparedRegionActivation,
    region_service::{PreparedRegionSources, RegionService},
    saves::SaveHandle,
    simulation::SimulationWorker,
};
use bace_db_postgres::PgStore;
use bace_gameplay_api::GeneratorSpawnKey;
use bace_simulation::{
    Command, GeneratorAction, GeneratorCommand, GeneratorCommandOutcome, GeneratorHostRequest,
    GeneratorServiceError,
};
use bace_types::EntityId;
pub use repository::GeneratorRepository;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
const PREFIX: u64 = 0x4745_0000_0000_0000;
const MAX_BYTES: usize = 64 * 1024 * 1024;
pub struct GeneratorServiceConfig {
    pub world_epoch: u64,
    pub capacity: usize,
    pub assets: crate::region_activation::RegionAssetManifest,
    pub generation: Arc<bace_storage_codec::PackGeneration>,
    pub treasure_assets: Arc<bace_loot::TreasureAssets>,
    pub random: Arc<bace_random::RandomRoot>,
    pub drop_plain_wield: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Materialize,
    Ids,
    Bind,
    Ready,
    Refresh,
    Complete,
}
struct Work {
    publications: Vec<PreparedGeneratorPublication>,
    request: GeneratorHostRequest,
    region: Option<Arc<PreparedRegionActivation>>,
    phase: Phase,
    raw: Option<Arc<materialization::Materialized>>,
    bytes: usize,
    ids: Vec<EntityId>,
    ready: Option<materialization::Ready>,
    sources: Option<PreparedRegionSources>,
    source_ids: Vec<EntityId>,
    failed_sources: Vec<EntityId>,
    registered: bool,
    delivery: Option<delivery::Delivery>,
    blocked: Option<String>,
    slot_refreshed: bool,
}
pub struct GeneratorService<S: GeneratorRepository = PgStore> {
    publications: BTreeMap<EntityId, PreparedGeneratorPublication>,
    publication_bytes: usize,
    publication_count: usize,
    store: S,
    config: GeneratorServiceConfig,
    cold: cold::Worker,
    jobs: BTreeMap<GeneratorSpawnKey, Work>,
    /// One contained birth per owner until its exact admission receipt. A later
    /// birth must refresh the current slot image after the earlier one commits.
    contained_owners: BTreeMap<EntityId, GeneratorSpawnKey>,
    cold_key: Option<GeneratorSpawnKey>,
    spare_ids: VecDeque<EntityId>,
    seed: Option<delivery::Delivery>,
    discard: Option<delivery::Delivery>,
    discarded: bool,
    next: u64,
    cursor: Option<GeneratorSpawnKey>,
    quiescing: bool,
    bytes: usize,
    fault: Option<String>,
    unexpected_request: Option<GeneratorHostRequest>,
    unexpected_cold: Option<cold::Completion>,
    unexpected_outcome: Option<GeneratorCommandOutcome>,
    retirement: Option<retirement::Retiring>,
}
pub enum GeneratorShutdownError<S: GeneratorRepository = PgStore> {
    Pending(Box<GeneratorService<S>>),
    Worker(String),
}
pub struct GeneratorShutdown {
    pub unused_allocated_ids: Vec<EntityId>,
}
impl<S: GeneratorRepository> GeneratorService<S> {
    pub fn start(store: S, config: GeneratorServiceConfig) -> Result<Self, String> {
        if config.world_epoch == 0
            || config.world_epoch > i64::MAX as u64
            || !(1..=64).contains(&config.capacity)
        {
            return Err("invalid generator service bounds".into());
        }
        let cold = cold::Worker::start(&config)?;
        Ok(Self {
            publications: BTreeMap::new(),
            publication_bytes: 0,
            publication_count: 0,
            store,
            config,
            cold,
            jobs: BTreeMap::new(),
            contained_owners: BTreeMap::new(),
            cold_key: None,
            spare_ids: VecDeque::new(),
            seed: None,
            discard: None,
            discarded: false,
            next: PREFIX,
            cursor: None,
            quiescing: false,
            bytes: 0,
            fault: None,
            unexpected_request: None,
            unexpected_cold: None,
            unexpected_outcome: None,
            retirement: None,
        })
    }
    pub fn quiesce(&mut self) {
        self.quiescing = true;
    }
    pub fn blocked(&self) -> impl Iterator<Item = (GeneratorSpawnKey, &str)> {
        self.jobs
            .iter()
            .filter_map(|(key, w)| w.blocked.as_deref().map(|e| (*key, e)))
    }
    pub fn failure(&self) -> Option<&str> {
        self.fault
            .as_deref()
            .or_else(|| self.retirement.as_ref().and_then(|r| r.failure()))
    }
    pub fn publication(&self, entity: EntityId) -> Option<&PreparedGeneratorPublication> {
        self.publications.get(&entity)
    }
    pub fn acknowledge_publication(
        &mut self,
        key: GeneratorSpawnKey,
        entity: EntityId,
    ) -> Result<(), String> {
        let publication = self
            .publications
            .get(&entity)
            .ok_or("generator publication missing")?;
        if publication.key != key {
            return Err("generator publication incarnation changed".into());
        }
        let bytes = publication.retained_bytes;
        self.publications.remove(&entity);
        self.publication_bytes -= bytes;
        self.publication_count -= 1;
        Ok(())
    }
    pub fn retry(&mut self, key: GeneratorSpawnKey) -> Result<(), String> {
        let work = self
            .jobs
            .get_mut(&key)
            .ok_or("unknown retained generator request")?;
        if work.phase == Phase::Ready && work.blocked.is_some() {
            work.phase = Phase::Refresh;
            work.delivery = None;
        }
        work.blocked = None;
        Ok(())
    }
    pub fn retry_lifecycle(&mut self) {
        self.fault = None;
        if let Some(r) = &mut self.retirement {
            r.retry();
        }
    }
    pub fn has_pending(&self) -> bool {
        !self.publications.is_empty()
            || !self.jobs.is_empty()
            || !self.contained_owners.is_empty()
            || self.cold_key.is_some()
            || self.seed.is_some()
            || self.discard.is_some()
            || self.retirement.is_some()
            || self.fault.is_some()
            || self.unexpected_cold.is_some()
            || self.unexpected_request.is_some()
            || self.unexpected_outcome.is_some()
    }
    pub fn shutdown(self) -> Result<GeneratorShutdown, GeneratorShutdownError<S>> {
        if !self.quiescing || !self.discarded || self.has_pending() {
            return Err(GeneratorShutdownError::Pending(Box::new(self)));
        }
        let ids = self.spare_ids.into_iter().collect();
        self.cold
            .shutdown()
            .map_err(GeneratorShutdownError::Worker)?;
        Ok(GeneratorShutdown {
            unused_allocated_ids: ids,
        })
    }
    /// Correlation namespace is reserved for this single runtime owner. The outer
    /// loop routes each generator outcome once, including failures and duplicates.
    pub fn owns_generator_outcome(&self, outcome: &GeneratorCommandOutcome) -> bool {
        outcome.correlation >> 48 == PREFIX >> 48
    }
    pub fn accept_generator_outcome(
        &mut self,
        outcome: GeneratorCommandOutcome,
    ) -> Result<(), Box<GeneratorCommandOutcome>> {
        self.accept_outcome(outcome)
    }
    /// Observe accepted lifecycle output before forwarding it to replication.
    pub fn observe_generator_event(
        &mut self,
        event: &bace_simulation::GeneratorWorldEvent,
        regions: &mut impl GeneratorRegions,
    ) -> Result<(), String> {
        use bace_gameplay_api::GeneratorLifecycleEffect as E;
        if let bace_simulation::GeneratorWorldEvent::Lifecycle(effect) = event {
            match effect {
                E::DestroyMember { member, .. } => regions.forget_transient_tree(member.entity),
                E::DetachMember { entity, .. } => regions.forget_transient_tree(*entity),
                _ => {}
            }
        }
        Ok(())
    }
    /// One bounded DB allocation/read and cold preparation transition per active
    /// request per poll. No database or DAT work executes on the simulation thread.
    pub async fn poll(
        &mut self,
        worker: &SimulationWorker,
        regions: &mut impl GeneratorRegions,
        saves: &SaveHandle,
    ) -> Result<(), String> {
        if self.fault.is_some()
            || self.unexpected_cold.is_some()
            || self.unexpected_request.is_some()
            || self.unexpected_outcome.is_some()
        {
            return Ok(());
        }
        if let Err(e) = self.poll_inner(worker, regions, saves).await {
            self.fault = Some(e);
        }
        Ok(())
    }
    async fn poll_inner(
        &mut self,
        worker: &SimulationWorker,
        regions: &mut impl GeneratorRegions,
        saves: &SaveHandle,
    ) -> Result<(), String> {
        self.poll_retirement(worker, regions, saves).await?;
        if self.cold_key.is_some()
            && let Some(completed) = self.cold.receive()?
        {
            if self.cold_key != Some(completed.key) || !self.jobs.contains_key(&completed.key) {
                self.unexpected_cold = Some(completed);
                return Err("generator cold completion key mismatch".into());
            }
            self.cold_key = None;
            let work = self
                .jobs
                .get_mut(&completed.key)
                .ok_or("generator cold request missing")?;
            match completed.value {
                Ok(cold::Value::Materialized { raw, bytes }) => {
                    self.bytes += bytes;
                    work.bytes = bytes;
                    work.raw = Some(raw);
                    work.phase = Phase::Ids;
                }
                Ok(cold::Value::Bound {
                    ready,
                    batch,
                    publications,
                }) => {
                    let added = publications
                        .iter()
                        .try_fold(0usize, |total, publication| {
                            total.checked_add(publication.retained_bytes)
                        })
                        .ok_or("generator publication byte overflow")?;
                    if self
                        .publication_bytes
                        .checked_add(added)
                        .is_none_or(|total| total > MAX_BYTES)
                        || self
                            .publication_count
                            .checked_add(publications.len())
                            .is_none_or(|total| total > 4096)
                        || publications
                            .iter()
                            .any(|publication| self.publications.contains_key(&publication.entity))
                    {
                        work.blocked = Some("generator publication capacity or identity".into());
                    } else {
                        self.publication_bytes += added;
                        self.publication_count += publications.len();
                        work.publications = publications;
                        work.source_ids = batch.ids().map(EntityId).collect();
                        work.sources = Some(batch);
                        work.ready = Some(*ready);
                        work.phase = Phase::Ready;
                        work.registered = false;
                    }
                }
                Err(e) => work.blocked = Some(e),
            }
        }
        for _ in 0..8 {
            if self.jobs.len() >= self.config.capacity {
                break;
            }
            let request = match worker.generator_requests().try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if self.quiescing {
                        break;
                    }
                    return Err("generator request lane disconnected".into());
                }
            };
            self.accept_request(request)?;
        }
        let completed: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, w)| w.phase == Phase::Complete)
            .map(|(&k, _)| k)
            .collect();
        for key in completed {
            if let Some(bace_gameplay_api::GeneratorDestination::Contain { container }) = self
                .jobs
                .get(&key)
                .map(|work| work.request.intent.destination)
                && self.contained_owners.get(&container) != Some(&key)
            {
                return Err("contained generator lease mismatch".into());
            }
            let work = self
                .jobs
                .remove(&key)
                .ok_or("completed generator missing")?;
            if let bace_gameplay_api::GeneratorDestination::Contain { container } =
                work.request.intent.destination
            {
                let _ = self.contained_owners.remove(&container);
            }
            self.bytes -= work.bytes;
            for publication in work.publications {
                if work.failed_sources.contains(&publication.entity) {
                    self.publication_bytes -= publication.retained_bytes;
                    self.publication_count -= 1;
                    continue;
                }
                if self
                    .publications
                    .insert(publication.entity, publication)
                    .is_some()
                {
                    return Err("generator publication identity collision".into());
                }
            }
            regions.forget_transient_sources(&work.failed_sources);
        }
        self.seed_ids(worker).await?;
        let next = self
            .jobs
            .iter()
            .filter(|(_, w)| w.blocked.is_none())
            .find(|(k, _)| self.cursor.is_none_or(|last| **k > last))
            .or_else(|| self.jobs.iter().find(|(_, w)| w.blocked.is_none()))
            .map(|(&key, _)| key);
        if let Some(key) = next {
            self.cursor = Some(key);
            if let Err(e) = self.advance_work(key, worker, regions).await {
                self.jobs
                    .get_mut(&key)
                    .ok_or("generator work disappeared")?
                    .blocked = Some(e);
            }
        }
        if self.quiescing
            && self.jobs.is_empty()
            && self.retirement.is_none()
            && self.seed.is_none()
            && !self.discarded
        {
            if self.discard.is_none() {
                self.discard = Some(self.delivery(
                    GeneratorAction::DiscardUnusedIdsAfterDrain,
                    delivery::Purpose::Discard,
                )?);
            }
            if let Some(delivery) = &mut self.discard {
                delivery.submit(worker);
            }
        }
        Ok(())
    }
    fn accept_request(&mut self, request: GeneratorHostRequest) -> Result<(), String> {
        let key = request.intent.key;
        if request.entities.is_empty()
            || request.entities.len() > 1024
            || key.generator.content_revision == 0
        {
            self.unexpected_request = Some(request);
            return Err("invalid generator host request bounds".into());
        }
        if let Some(work) = self.jobs.get_mut(&key) {
            if work.request.intent != request.intent
                || work.request.landblock != request.landblock
                || !request.entities.starts_with(&work.request.entities)
                || (!matches!(work.phase, Phase::Refresh | Phase::Ids))
            {
                self.unexpected_request = Some(request);
                return Err("unexpected generator request refresh".into());
            }
            work.request = request;
            work.phase = if work.raw.is_some() {
                Phase::Ids
            } else {
                Phase::Materialize
            };
        } else if self.jobs.len() < self.config.capacity {
            self.jobs.insert(
                key,
                Work {
                    publications: vec![],
                    request,
                    region: None,
                    phase: Phase::Materialize,
                    raw: None,
                    bytes: 0,
                    ids: vec![],
                    ready: None,
                    sources: None,
                    source_ids: vec![],
                    failed_sources: vec![],
                    registered: false,
                    delivery: None,
                    blocked: None,
                    slot_refreshed: false,
                },
            );
        } else {
            self.unexpected_request = Some(request);
            return Err("generator service refresh lane capacity".into());
        }
        Ok(())
    }
    async fn seed_ids(&mut self, worker: &SimulationWorker) -> Result<(), String> {
        if self.quiescing {
            if self.seed.as_ref().is_some_and(|d| !d.submitted) {
                self.seed = None;
            }
            return Ok(());
        }
        if self.jobs.len() >= self.config.capacity {
            return Ok(());
        }
        if self.seed.is_none() {
            if self.spare_ids.is_empty() {
                let ids = self.store.allocate(8).await.map_err(|e| e.to_string())?;
                self.spare_ids.extend(ids.into_iter().map(EntityId));
            }
            let id = *self
                .spare_ids
                .front()
                .ok_or("allocator returned no generator identity")?;
            self.seed =
                Some(self.delivery(GeneratorAction::SupplyId(id), delivery::Purpose::Seed)?);
        }
        if let Some(delivery) = &mut self.seed {
            delivery.submit(worker);
        }
        Ok(())
    }
    async fn advance_work(
        &mut self,
        key: GeneratorSpawnKey,
        worker: &SimulationWorker,
        regions: &mut impl GeneratorRegions,
    ) -> Result<(), String> {
        if self.jobs[&key].region.is_none() {
            let region = regions
                .prepared_region(self.jobs[&key].request.landblock)
                .ok_or("generator source region not accepted")?
                .clone();
            if region.fence.content_generation != key.generator.content_revision {
                return Err("generator source content fence mismatch".into());
            }
            self.jobs.get_mut(&key).unwrap().region = Some(region);
        }
        if self.jobs[&key].delivery.is_some() {
            self.jobs
                .get_mut(&key)
                .unwrap()
                .delivery
                .as_mut()
                .unwrap()
                .submit(worker);
            return Ok(());
        }
        let phase = self.jobs[&key].phase;
        if matches!(phase, Phase::Bind | Phase::Ready | Phase::Refresh)
            && let bace_gameplay_api::GeneratorDestination::Contain { container } =
                self.jobs[&key].request.intent.destination
        {
            match self.contained_owners.get(&container).copied() {
                Some(owner) if owner != key => return Ok(()),
                Some(_) => {}
                None => {
                    self.contained_owners.insert(container, key);
                    self.jobs.get_mut(&key).unwrap().slot_refreshed = false;
                }
            }
        }
        match phase {
            Phase::Materialize | Phase::Bind => {
                if phase == Phase::Bind
                    && !self.jobs[&key].slot_refreshed
                    && matches!(
                        self.jobs[&key].request.intent.destination,
                        bace_gameplay_api::GeneratorDestination::Contain { .. }
                    )
                {
                    let d = self.delivery(
                        GeneratorAction::RefreshRequest(key),
                        delivery::Purpose::Refresh(key),
                    )?;
                    self.jobs.get_mut(&key).unwrap().delivery = Some(d);
                    return Ok(());
                }
                if self.cold_key.is_some() {
                    return Ok(());
                }
                let work = &self.jobs[&key];
                let region = work
                    .region
                    .as_ref()
                    .ok_or("missing generator source region")?
                    .clone();
                let job = if phase == Phase::Materialize {
                    cold::Job::Materialize {
                        request: work.request.clone(),
                        region,
                        budget: MAX_BYTES - self.bytes,
                    }
                } else {
                    cold::Job::Bind {
                        request: work.request.clone(),
                        region,
                        raw: work
                            .raw
                            .as_ref()
                            .ok_or("missing retained materialization")?
                            .clone(),
                    }
                };
                if self.cold.submit(job) {
                    self.cold_key = Some(key);
                }
            }
            Phase::Ids => {
                let total = self.jobs[&key]
                    .raw
                    .as_ref()
                    .ok_or("missing materialization")?
                    .count();
                let expected = self.jobs[&key].request.entities.len();
                if total == 0 || total == expected {
                    self.jobs.get_mut(&key).unwrap().phase = Phase::Bind;
                    return Ok(());
                }
                if total < expected || total > 1024 {
                    return Err("generator materialization identity count changed".into());
                }
                if self.jobs[&key].ids.is_empty() {
                    let ids = self
                        .store
                        .allocate((total - expected) as u16)
                        .await
                        .map_err(|e| e.to_string())?;
                    self.jobs.get_mut(&key).unwrap().ids = ids.into_iter().map(EntityId).collect();
                }
                let ids = self.jobs[&key].ids.clone();
                let d = self.delivery(
                    GeneratorAction::SupplyRequestIds { key, expected, ids },
                    delivery::Purpose::Reserve(key),
                )?;
                self.jobs.get_mut(&key).unwrap().delivery = Some(d);
            }
            Phase::Ready => {
                let work = self.jobs.get_mut(&key).unwrap();
                if !work.registered {
                    if let Some(batch) = work.sources.take()
                        && let Err(batch) = regions.record_sources(work.request.landblock, batch)
                    {
                        work.sources = Some(*batch);
                        return Ok(());
                    }
                    work.registered = true;
                }
                let action = work
                    .ready
                    .as_ref()
                    .ok_or("missing generated admission")?
                    .action
                    .clone();
                let d = self.delivery(action, delivery::Purpose::Admit(key))?;
                self.jobs.get_mut(&key).unwrap().delivery = Some(d);
            }
            Phase::Refresh => {
                let d = self.delivery(
                    GeneratorAction::RefreshRequest(key),
                    delivery::Purpose::Refresh(key),
                )?;
                self.jobs.get_mut(&key).unwrap().delivery = Some(d);
            }
            Phase::Complete => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
