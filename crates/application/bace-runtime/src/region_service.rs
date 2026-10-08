//! Bounded production region lifecycle. Cold work and PostgreSQL reads stay off
//! simulation. Failed/uncertain work retains immutable inputs and exact fences.
mod clock;
mod cold;
mod equipment;
mod npc_sources;
mod reconciliation;
mod refresh;
mod sources;
pub use sources::PreparedRegionSources;
mod unload;
pub(crate) mod world_items;
use crate::{
    placement_saves::{PendingPlacementSave, PlacementResolution},
    region_activation::{
        PreparedRegionActivation, RegionActivationRequest, RegionActivationWorker,
    },
    saves::SaveHandle,
    simulation::SimulationWorker,
};
use bace_db_postgres::PgStore;
use bace_persistence::{InventoryLoadLimits, LocatedSnapshot};
use bace_simulation::{PreparedResidentRegion, RegionLifecycleEvent};
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
#[derive(Clone, Copy, Debug)]
pub struct RegionClock {
    pub unix_seconds: i64,
    pub tick: u64,
    pub portal_seconds: f64,
}
pub struct RegionServiceConfig {
    pub world_epoch: u64,
    /// Must be at least the configured simulation residency capacity.
    pub capacity: usize,
    pub assets: crate::region_activation::RegionAssetManifest,
    pub generation: Arc<bace_storage_codec::PackGeneration>,
    pub creature_policy: crate::generator_preparation::CreatureAdmissionPolicy,
    pub treasure_assets: Arc<bace_loot::TreasureAssets>,
    pub random: Arc<bace_random::RandomRoot>,
    pub content_hash: [u8; 32],
    pub aetheria_drop_rate: f32,
    pub options: crate::generator_preparation::GeneratorPreparationOptions,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Assets,
    NpcSources,
    Cells,
    Reconcile,
    EncounterIds,
    Gear,
    GearIds,
    Finish,
    Ready,
    Submitted,
}
struct Loading {
    npc_after: Option<u32>,
    npc_heads: VecDeque<bace_db_postgres::StoredNpcSourceHead>,
    npc_request: Option<Arc<crate::npc_region::NpcRegionRequest>>,
    npc_bytes: usize,
    views: Vec<crate::visibility_assets::PreparedVisibilityObject>,
    npcs: Vec<crate::npc_sources::PreparedNpcRegistration>,
    bindings: Vec<(EntityId, bace_interactions::BindingKind)>,
    request: RegionActivationRequest,
    prepared: Option<Arc<PreparedRegionActivation>>,
    phase: Phase,
    submitted: bool,
    cells: Vec<u32>,
    cell_index: usize,
    snapshots: Vec<LocatedSnapshot>,
    restore: Arc<Vec<LocatedSnapshot>>,
    retirement: Vec<PendingPlacementSave>,
    retirement_next: usize,
    encounters: BTreeMap<u32, EntityId>,
    gear: Option<Arc<equipment::RegionEquipment>>,
    ids: Vec<EntityId>,
    ready: Option<cold::Finished>,
    sources: BTreeMap<u32, crate::region_unload_saves::RegionItemSource>,
    source_bytes: BTreeMap<u32, usize>,
    restore_clock: Option<RegionClock>,
}
pub struct RegionService {
    refresh: refresh::RefreshState,
    refresh_worker: refresh::Worker,
    npc_directory: Option<Arc<std::path::PathBuf>>,
    visibility_sources: VecDeque<(u64, crate::visibility_assets::PreparedVisibilityObject)>,
    visibility_retirements: VecDeque<(EntityId, u64)>,
    npc_pending: Arc<std::collections::BTreeSet<EntityId>>,
    npc_suppressed: Arc<std::collections::BTreeSet<EntityId>>,
    npc_sources: VecDeque<crate::npc_sources::PreparedNpcRegistration>,
    binding_notices: VecDeque<(EntityId, bace_interactions::BindingKind, bool)>,
    active_bindings: BTreeMap<u16, Vec<(EntityId, bace_interactions::BindingKind)>>,
    store: PgStore,
    config: RegionServiceConfig,
    activation: RegionActivationWorker,
    cold: cold::Worker,
    pending: VecDeque<(u16, u64)>,
    known: BTreeMap<u16, u64>,
    active: BTreeMap<u16, Arc<PreparedRegionActivation>>,
    loading: Option<Loading>,
    sources: BTreeMap<u32, crate::region_unload_saves::RegionItemSource>,
    unload: Option<unload::Unloading>,
    blocked: BTreeMap<u16, String>,
    source_bytes: BTreeMap<u32, usize>,
    source_regions: BTreeMap<u32, u16>,
    next: u64,
    clock: clock::ClockDelivery,
    held_event: Option<RegionLifecycleEvent>,
    unexpected: Option<bace_simulation::RegionAdmissionOutcome>,
}
pub enum RegionServiceShutdownError {
    Pending(Box<RegionService>),
    Worker(String),
}
impl RegionService {
    pub(crate) fn treasure_assets(&self) -> Arc<bace_loot::TreasureAssets> {
        self.config.treasure_assets.clone()
    }
    pub(crate) fn aetheria_drop_rate(&self) -> f32 {
        self.config.aetheria_drop_rate
    }

    pub fn note_npc_archive(&mut self, source: EntityId) -> Result<(), String> {
        if source.0 == 0
            || self.npc_suppressed.len() == 65536 && !self.npc_suppressed.contains(&source)
        {
            return Err("NPC archived identity capacity".into());
        }
        Arc::make_mut(&mut self.npc_suppressed).insert(source);
        Ok(())
    }

    pub fn take_visibility_source(
        &mut self,
    ) -> Option<(u64, crate::visibility_assets::PreparedVisibilityObject)> {
        self.visibility_sources.pop_front()
    }
    pub fn visibility_retirement(&self) -> Option<(EntityId, u64)> {
        self.visibility_retirements.front().copied()
    }
    pub fn acknowledge_visibility_retirement(
        &mut self,
        expected: (EntityId, u64),
    ) -> Result<(), String> {
        if self.visibility_retirements.front().copied() != Some(expected) {
            return Err("region visibility retirement correlation".into());
        }
        self.visibility_retirements.pop_front();
        Ok(())
    }

    pub fn set_npc_history_directory(
        &mut self,
        directory: std::path::PathBuf,
    ) -> Result<(), String> {
        if self.loading.is_some() || !self.active.is_empty() {
            return Err("NPC history configuration after admission".into());
        }
        self.npc_directory = Some(Arc::new(directory));
        Ok(())
    }
    /// Startup loads this global inventory before any regional body admission.
    pub fn set_npc_recovery_inventory(
        &mut self,
        pending: Arc<std::collections::BTreeSet<EntityId>>,
        archived: Arc<std::collections::BTreeSet<EntityId>>,
    ) -> Result<(), String> {
        if !self.active.is_empty()
            || self.loading.is_some()
            || pending.len() > 65536
            || archived.len() > 65536
        {
            return Err("NPC startup inventory after region activation".into());
        }
        self.npc_pending = pending;
        self.npc_suppressed = archived;
        Ok(())
    }
    pub fn npc_recovery_inventory(
        &self,
    ) -> (
        Arc<std::collections::BTreeSet<EntityId>>,
        Arc<std::collections::BTreeSet<EntityId>>,
    ) {
        (self.npc_pending.clone(), self.npc_suppressed.clone())
    }
    pub fn take_npc_source(&mut self) -> Option<crate::npc_sources::PreparedNpcRegistration> {
        self.npc_sources.pop_front()
    }

    pub fn binding_notice(&self) -> Option<(EntityId, bace_interactions::BindingKind, bool)> {
        self.binding_notices.front().copied()
    }
    pub fn binding_notice_pending(&self, object: EntityId) -> bool {
        self.binding_notices.iter().any(|(id, _, _)| *id == object)
    }
    pub fn acknowledge_binding_notice(
        &mut self,
        expected: (EntityId, bace_interactions::BindingKind, bool),
    ) -> Result<(), String> {
        if self.binding_notices.front().copied() != Some(expected) {
            return Err("binding region delivery correlation".into());
        }
        self.binding_notices.pop_front();
        Ok(())
    }

    pub fn content_generation(&self) -> u64 {
        self.config.generation.revision()
    }
    /// Adopt the accepted source for cold activations and schedule occupied
    /// authored regions for bounded owner-fenced refresh.
    pub fn adopt_content_generation(
        &mut self,
        generation: Arc<bace_storage_codec::PackGeneration>,
        content_hash: [u8; 32],
        changed_keys: &[bace_storage_codec::PackKey],
    ) -> Result<(), String> {
        if generation.revision() < self.config.generation.revision() || content_hash == [0; 32] {
            return Err("stale or invalid region content generation".into());
        }
        self.config.generation = generation;
        self.config.content_hash = content_hash;
        let changed: std::collections::BTreeSet<_> = changed_keys.iter().copied().collect();
        let global_changed = refresh::global_changed(&changed);
        let blocks: std::collections::BTreeSet<_> = self
            .active
            .iter()
            .filter_map(|(&block, region)| {
                refresh::affected(region, &changed, global_changed).then_some(block)
            })
            .collect();
        self.refresh
            .accepted_targets(&mut self.blocked, blocks, self.config.capacity);
        Ok(())
    }
    /// Stop only after the simulation/save drain. Pending work returns the whole
    /// live service, including reservations, workers and immutable retry inputs.
    pub fn shutdown(self) -> Result<(), RegionServiceShutdownError> {
        if self.has_pending() {
            return Err(RegionServiceShutdownError::Pending(Box::new(self)));
        }
        let activation = self.activation.shutdown();
        let cold = self.cold.shutdown();
        let refresh = self.refresh_worker.shutdown();
        match (activation, cold, refresh) {
            (Ok(a), Ok(c), Ok(r)) if a.is_empty() && c.is_empty() && r == 0 => Ok(()),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                Err(RegionServiceShutdownError::Worker(e))
            }
            _ => Err(RegionServiceShutdownError::Worker(
                "unexpected idle region worker completion".into(),
            )),
        }
    }

    pub fn start(store: PgStore, config: RegionServiceConfig) -> Result<Self, String> {
        if config.world_epoch == 0
            || !(1..=4096).contains(&config.capacity)
            || !config.aetheria_drop_rate.is_finite()
        {
            return Err("invalid region service bounds".into());
        }
        let activation = RegionActivationWorker::start(config.assets.clone(), 1)?;
        let cold = cold::Worker::start(config.assets.clone())?;
        let refresh_worker = refresh::Worker::start(config.assets.clone())?;
        Ok(Self {
            refresh: refresh::RefreshState::default(),
            refresh_worker,
            visibility_sources: VecDeque::new(),
            visibility_retirements: VecDeque::new(),
            npc_directory: None,
            npc_pending: Arc::default(),
            npc_suppressed: Arc::default(),
            npc_sources: VecDeque::new(),
            binding_notices: VecDeque::new(),
            active_bindings: BTreeMap::new(),
            store,
            config,
            activation,
            cold,
            pending: VecDeque::new(),
            known: BTreeMap::new(),
            active: BTreeMap::new(),
            loading: None,
            sources: BTreeMap::new(),
            unload: None,
            blocked: BTreeMap::new(),
            source_bytes: BTreeMap::new(),
            source_regions: BTreeMap::new(),
            next: 0,
            clock: Default::default(),
            held_event: None,
            unexpected: None,
        })
    }
    pub fn blocked(&self) -> impl Iterator<Item = (u16, &str)> {
        self.blocked.iter().map(|(&b, e)| (b, e.as_str()))
    }
    pub fn has_pending(&self) -> bool {
        !self.visibility_sources.is_empty()
            || !self.visibility_retirements.is_empty()
            || !self.npc_sources.is_empty()
            || !self.binding_notices.is_empty()
            || self.loading.is_some()
            || self.unload.is_some()
            || !self.pending.is_empty()
            || self.held_event.is_some()
            || self.unexpected.is_some()
            || self.clock.has_pending()
            || self.refresh.has_pending()
    }
    pub fn prepared_region(&self, landblock: u16) -> Option<&Arc<PreparedRegionActivation>> {
        self.active.get(&landblock)
    }
    /// Only retries retained inputs; it never resets IDs, RNG identity or expiry.
    pub fn retry(&mut self, landblock: u16) {
        self.blocked.remove(&landblock);
        if self.active.contains_key(&landblock) && !self.refresh.current_for(landblock) {
            self.refresh.retry(landblock, self.config.capacity);
        }
    }
    pub fn record_source(
        &mut self,
        landblock: u16,
        mut source: crate::region_unload_saves::RegionItemSource,
    ) -> Result<(), Box<crate::region_unload_saves::RegionItemSource>> {
        if self
            .loading
            .as_ref()
            .is_some_and(|l| l.phase == Phase::Submitted)
        {
            return Err(Box::new(source));
        }
        source.item.corpse = source.corpse.clone().map(Box::new);
        let id = source.item.entity.object_id;
        let Ok(bytes) = world_items::source_size(&source) else {
            return Err(Box::new(source));
        };
        let used = self.source_bytes.values().sum::<usize>()
            - self.source_bytes.get(&id).copied().unwrap_or(0);
        if self.sources.len() >= 4096 && !self.sources.contains_key(&id)
            || used + bytes > 64 * 1024 * 1024
        {
            return Err(Box::new(source));
        }
        self.source_regions.insert(id, landblock);
        self.source_bytes.insert(id, bytes);
        self.sources.insert(id, source);
        Ok(())
    }
    /// Drive from one retained runtime service future. A poll processes one DB
    /// cell/read batch and at most eight lifecycle events, never a tick-thread wait.
    pub async fn poll(
        &mut self,
        worker: &SimulationWorker,
        saves: &SaveHandle,
        clock: RegionClock,
    ) -> Result<(), String> {
        if clock.unix_seconds < 0 {
            return Err("invalid region clock".into());
        }
        self.poll_clock(worker, clock)?;
        self.poll_unload(worker, saves).await?;
        for _ in 0..8 {
            let event = match self
                .held_event
                .take()
                .or_else(|| worker.region_lifecycle_events().try_recv().ok())
            {
                Some(e) => e,
                None => break,
            };
            match event {
                RegionLifecycleEvent::Prepare { landblock, epoch } => {
                    if self.known.get(&landblock).is_some_and(|old| *old >= epoch) {
                        continue;
                    }
                    if self.known.len() >= self.config.capacity
                        && !self.known.contains_key(&landblock)
                    {
                        self.held_event = Some(event);
                        break;
                    }
                    self.known.insert(landblock, epoch);
                    self.pending.push_back((landblock, epoch));
                }
                RegionLifecycleEvent::Unloaded { landblock, epoch } => {
                    if self.known.get(&landblock) == Some(&epoch) {
                        let binding_count =
                            self.active_bindings.get(&landblock).map_or(0, Vec::len);
                        if binding_count > 4096 - self.binding_notices.len() {
                            self.held_event = Some(event);
                            break;
                        }
                        if let Some(bindings) = self.active_bindings.remove(&landblock) {
                            self.binding_notices
                                .extend(bindings.into_iter().map(|(id, kind)| (id, kind, false)));
                        }
                        self.known.remove(&landblock);
                        self.active.remove(&landblock);
                        self.refresh.unloaded(landblock);
                        self.blocked.remove(&landblock);
                    }
                    self.finish_unload(landblock, epoch);
                    let retired: Vec<_> = self
                        .source_regions
                        .iter()
                        .filter_map(|(&id, &block)| (block == landblock).then_some(id))
                        .collect();
                    for id in retired {
                        self.source_regions.remove(&id);
                        self.source_bytes.remove(&id);
                        self.sources.remove(&id);
                    }
                }
                _ => {}
            }
        }
        self.poll_admission(worker)?;
        self.poll_refresh(worker)?;
        if self.loading.is_none() {
            if self.refresh.submitted() {
                return Ok(());
            }
            if let Some((landblock, epoch)) = self.pending.pop_front() {
                self.next = self
                    .next
                    .checked_add(1)
                    .ok_or("region correlation exhausted")?;
                let request = RegionActivationRequest {
                    token: self.next,
                    activation_epoch: epoch,
                    landblock,
                    generation: self.config.generation.clone(),
                    creature_policy: Some(self.config.creature_policy),
                    treasure_assets: Some(self.config.treasure_assets.clone()),
                    content_hash: self.config.content_hash,
                    aetheria_drop_rate: self.config.aetheria_drop_rate,
                };
                self.loading = Some(Loading {
                    npc_after: None,
                    npc_heads: VecDeque::new(),
                    npc_request: None,
                    npc_bytes: 0,
                    views: vec![],
                    npcs: vec![],
                    bindings: vec![],
                    request,
                    prepared: None,
                    phase: Phase::Assets,
                    submitted: false,
                    cells: vec![],
                    cell_index: 0,
                    snapshots: vec![],
                    restore: Arc::new(vec![]),
                    retirement: vec![],
                    retirement_next: 0,
                    encounters: BTreeMap::new(),
                    gear: None,
                    ids: vec![],
                    ready: None,
                    sources: BTreeMap::new(),
                    source_bytes: BTreeMap::new(),
                    restore_clock: None,
                });
            } else {
                return Ok(());
            }
        }
        let block = self
            .loading
            .as_ref()
            .expect("loading created")
            .request
            .landblock;
        if self.blocked.contains_key(&block) {
            return Ok(());
        }
        if let Err(error) = self.advance_loading(worker, saves, clock).await {
            self.blocked.insert(block, error);
        }
        Ok(())
    }
    pub fn owns_generator_outcome(
        &self,
        outcome: &bace_simulation::GeneratorCommandOutcome,
    ) -> bool {
        clock::ClockDelivery::owns(outcome)
    }
    pub fn accept_generator_outcome(
        &mut self,
        outcome: bace_simulation::GeneratorCommandOutcome,
    ) -> Result<(), Box<bace_simulation::GeneratorCommandOutcome>> {
        self.clock.accept(outcome)
    }
    pub fn clock_failure(&self) -> Option<&bace_simulation::GeneratorCommandOutcome> {
        self.clock.failure()
    }
    pub fn retry_clock(&mut self) {
        self.clock.retry();
    }
    fn poll_clock(&mut self, worker: &SimulationWorker, clock: RegionClock) -> Result<(), String> {
        let day = crate::game_clock::generator_is_day(clock.portal_seconds)?;
        self.clock.update(day);
        self.clock
            .flush(|command| worker.input().try_submit(command).is_ok());
        Ok(())
    }
    fn poll_admission(&mut self, worker: &SimulationWorker) -> Result<(), String> {
        if self.refresh.submitted() {
            return self.poll_refresh_admission(worker);
        }
        if self.unexpected.is_some() {
            return Err("unexpected region admission awaits reconciliation".into());
        }
        let Some(current) = self
            .loading
            .as_mut()
            .filter(|l| l.phase == Phase::Submitted)
        else {
            return Ok(());
        };
        let Ok(outcome) = worker.region_admission_outcomes().try_recv() else {
            return Ok(());
        };
        if outcome.correlation != current.request.token {
            self.unexpected = Some(outcome);
            return Err("unexpected region admission correlation retained by service".into());
        }
        match outcome.result {
            Ok(()) => {
                let loaded = self.loading.take().expect("matched admission");
                self.npc_sources.extend(loaded.npcs);
                self.binding_notices
                    .extend(loaded.bindings.iter().map(|&(id, kind)| (id, kind, true)));
                self.active_bindings
                    .insert(loaded.request.landblock, loaded.bindings);
                self.visibility_sources
                    .extend(loaded.views.into_iter().map(|v| (outcome.tick, v)));
                self.active.insert(
                    loaded.request.landblock,
                    loaded.prepared.expect("prepared admission"),
                );
                self.refresh
                    .cold_admitted(&self.active[&loaded.request.landblock]);
                for (id, source) in loaded.sources {
                    self.source_bytes.insert(id, loaded.source_bytes[&id]);
                    self.source_regions.insert(id, loaded.request.landblock);
                    self.sources.insert(id, source);
                }
            }
            Err((error, input)) => {
                current.ready = Some(cold::Finished {
                    views: std::mem::take(&mut current.views),
                    npcs: std::mem::take(&mut current.npcs),
                    input: *input,
                    sources: std::mem::take(&mut current.sources),
                    source_bytes: std::mem::take(&mut current.source_bytes),
                });
                current.phase = Phase::Ready;
                current.submitted = false;
                self.blocked.insert(
                    current.request.landblock,
                    format!("region owner rejected: {error:?}"),
                );
            }
        }
        Ok(())
    }
    async fn advance_loading(
        &mut self,
        worker: &SimulationWorker,
        saves: &SaveHandle,
        clock: RegionClock,
    ) -> Result<(), String> {
        let l = self.loading.as_mut().expect("loading owner");
        match l.phase {
            Phase::Assets => {
                if !l.submitted {
                    if self.activation.try_submit(l.request.clone()).is_ok() {
                        l.submitted = true;
                    }
                    return Ok(());
                }
                let Ok(completed) = self.activation.try_recv() else {
                    return Ok(());
                };
                if completed.fence.token != l.request.token
                    || completed.fence.activation_epoch != l.request.activation_epoch
                    || completed.fence.landblock != l.request.landblock
                {
                    return Err("region activation fence mismatch".into());
                }
                l.submitted = false;
                let prepared = completed.result?;
                l.cells = prepared
                    .geometry
                    .cell_ids()
                    .filter(|id| id >> 16 == u32::from(l.request.landblock))
                    .collect();
                if l.cells.is_empty() || l.cells.len() > 1024 {
                    return Err("invalid region geometry cell count".into());
                }
                l.prepared = Some(Arc::new(prepared));
                l.phase = Phase::NpcSources;
            }
            Phase::NpcSources => {
                if l.submitted {
                    let Some(cold::Completion::PinNpc(result)) = self.cold.receive() else {
                        return Ok(());
                    };
                    l.submitted = false;
                    let pinned = result?;
                    if l.npc_heads
                        .front()
                        .is_none_or(|h| h.source != pinned.registration.actor.0)
                    {
                        return Err("NPC region pin completion identity".into());
                    }
                    let prepared = Arc::get_mut(l.prepared.as_mut().expect("prepared region"))
                        .ok_or("NPC regional closure still borrowed")?;
                    crate::npc_region::install(prepared, *pinned)?;
                    l.npc_heads.pop_front();
                    l.npc_request = None;
                    return Ok(());
                }
                if let Some(head) = l.npc_heads.front() {
                    if l.npc_request.is_none() {
                        let request = crate::npc_region::request(&self.store, head.clone()).await?;
                        let extra = request
                            .inventory
                            .iter()
                            .try_fold(
                                request.baseline.as_ref().map_or(0, |b| b.bytes.len()),
                                |n, i| n.checked_add(i.aggregate.bytes.len()),
                            )
                            .ok_or("NPC source inventory byte overflow")?;
                        let bytes = l
                            .npc_bytes
                            .checked_add(extra)
                            .ok_or("NPC regional source byte overflow")?;
                        if bytes > 64 * 1024 * 1024 {
                            return Err("NPC regional source inventory byte capacity".into());
                        }
                        l.npc_bytes = bytes;
                        l.npc_request = Some(Arc::new(request));
                        return Ok(());
                    }
                    let directory = self
                        .npc_directory
                        .clone()
                        .ok_or("NPC history directory not configured")?;
                    let job = cold::Job::PinNpc {
                        prepared: l.prepared.as_ref().unwrap().clone(),
                        request: l.request.clone(),
                        pin: l.npc_request.as_ref().unwrap().clone(),
                        directory,
                    };
                    if self.cold.submit(job).is_ok() {
                        l.submitted = true;
                    }
                    return Ok(());
                }
                let heads = self
                    .store
                    .npc_source_heads_in_region(l.request.landblock, l.npc_after, 16)
                    .await
                    .map_err(|e| e.to_string())?;
                if heads.is_empty() {
                    let ids: Vec<_> = l
                        .prepared
                        .as_ref()
                        .expect("prepared region")
                        .content
                        .instances
                        .iter()
                        .filter(|i| !i.source.is_link_child)
                        .map(|i| i.source.guid)
                        .collect();
                    let elsewhere: std::collections::BTreeSet<_> = self
                        .store
                        .npc_sources_outside_region(&ids, l.request.landblock)
                        .await
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .collect();
                    let prepared = Arc::get_mut(l.prepared.as_mut().expect("prepared region"))
                        .ok_or("NPC suppression source still borrowed")?;
                    prepared
                        .content
                        .instances
                        .retain(|i| i.source.is_link_child || !elsewhere.contains(&i.source.guid));
                    l.phase = Phase::Cells;
                    return Ok(());
                }
                for head in heads {
                    l.npc_bytes = l
                        .npc_bytes
                        .checked_add(head.checkpoint.len())
                        .ok_or("NPC regional source byte overflow")?;
                    if l.npc_bytes > 64 * 1024 * 1024
                        || l.npc_after.is_some_and(|old| head.source <= old)
                    {
                        return Err("NPC regional source page bounds".into());
                    }
                    l.npc_after = Some(head.source);
                    l.npc_heads.push_back(head);
                }
            }
            Phase::Cells => {
                if let Some(&cell) = l.cells.get(l.cell_index) {
                    let used = l
                        .snapshots
                        .iter()
                        .map(|s| s.aggregate.bytes.len())
                        .sum::<usize>();
                    let rows = self
                        .store
                        .load_world_item_tree(
                            cell,
                            InventoryLoadLimits {
                                max_items: 4096 - l.snapshots.len(),
                                max_depth: 64,
                                max_total_bytes: 64 * 1024 * 1024 - used,
                            },
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    l.snapshots.extend(rows);
                    l.cell_index += 1;
                    return Ok(());
                }
                // Classification is lossless on failure; never start generators
                // while any old generated root lacks a durable tombstone.
                let snapshots = std::mem::take(&mut l.snapshots);
                let recovered = match crate::generated_recovery::classify_generated_world_trees(
                    snapshots,
                    self.config.world_epoch,
                    limits(),
                ) {
                    Ok(v) => v,
                    Err((error, snapshots)) => {
                        l.snapshots = snapshots;
                        return Err(format!("region world recovery: {error:?}"));
                    }
                };
                let mut retirements = Vec::new();
                for tree in &recovered.held_for_retirement {
                    match reconciliation::freeze(tree) {
                        Ok(save) => retirements.push(save),
                        Err(error) => {
                            l.snapshots = recovered.restore_candidates;
                            l.snapshots.extend(
                                recovered
                                    .held_for_retirement
                                    .into_iter()
                                    .flat_map(|t| t.snapshots),
                            );
                            return Err(error);
                        }
                    }
                }
                l.restore = Arc::new(recovered.restore_candidates);
                l.retirement = retirements;
                l.phase = Phase::Reconcile;
            }
            Phase::Reconcile => {
                let Some(save) = l.retirement.get_mut(l.retirement_next) else {
                    l.phase = Phase::EncounterIds;
                    return Ok(());
                };
                if !l.submitted {
                    match save.submit(saves) {
                        Ok(()) => l.submitted = true,
                        Err(crate::saves::SaveSubmitError::Full) => {}
                        Err(e) => return Err(format!("region recovery submission: {e:?}")),
                    }
                    return Ok(());
                }
                if let Some(result) = save.poll() {
                    l.submitted = false;
                    match result {
                        PlacementResolution::Committed(_) => l.retirement_next += 1,
                        PlacementResolution::Uncertain(e) => {
                            return Err(format!("uncertain generated recovery: {e}"));
                        }
                        PlacementResolution::Rejected(e) => {
                            return Err(format!("generated recovery rejected: {e}"));
                        }
                    }
                }
            }
            Phase::EncounterIds => {
                let prepared = l.prepared.as_ref().expect("prepared region");
                let missing: Vec<_> = prepared
                    .content
                    .encounters
                    .iter()
                    .map(|e| e.source.id)
                    .filter(|id| !l.encounters.contains_key(id))
                    .take(1024)
                    .collect();
                if missing.is_empty() {
                    l.phase = Phase::Gear;
                    return Ok(());
                }
                let ids = self
                    .store
                    .allocate_dynamic_ids(missing.len() as u16)
                    .await
                    .map_err(|e| e.to_string())?;
                l.encounters
                    .extend(missing.into_iter().zip(ids.into_iter().map(EntityId)));
            }
            Phase::Gear => {
                if !l.submitted {
                    let job = cold::Job::Gear {
                        prepared: l.prepared.as_ref().unwrap().clone(),
                        encounters: l.encounters.clone(),
                        random: self.config.random.clone(),
                        world_epoch: self.config.world_epoch,
                        drop_plain_wield: self.config.options.drop_plain_wield,
                    };
                    if self.cold.submit(job).is_ok() {
                        l.submitted = true;
                    }
                    return Ok(());
                }
                let Some(cold::Completion::Gear(result)) = self.cold.receive() else {
                    return Ok(());
                };
                l.submitted = false;
                l.gear = Some(Arc::new(result?));
                l.phase = Phase::GearIds;
            }
            Phase::GearIds => {
                let count = l.gear.as_ref().unwrap().count;
                if l.ids.len() == count {
                    l.phase = Phase::Finish;
                    return Ok(());
                }
                let count = (count - l.ids.len()).min(1024);
                l.ids.extend(
                    self.store
                        .allocate_dynamic_ids(count as u16)
                        .await
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .map(EntityId),
                );
            }
            Phase::Finish => {
                if !l.submitted {
                    let job = cold::Job::Finish {
                        prepared: l.prepared.as_ref().unwrap().clone(),
                        encounters: l.encounters.clone(),
                        gear: l.gear.as_ref().unwrap().clone(),
                        ids: l.ids.clone(),
                        snapshots: l.restore.clone(),
                        clock: *l.restore_clock.get_or_insert(clock),
                        generation: l.request.generation.clone(),
                        options: self.config.options,
                        suppressed: self.npc_suppressed.clone(),
                        content_hash: l.request.content_hash,
                    };
                    if self.cold.submit(job).is_ok() {
                        l.submitted = true;
                    }
                    return Ok(());
                }
                let Some(cold::Completion::Finish(result)) = self.cold.receive() else {
                    return Ok(());
                };
                l.submitted = false;
                l.ready = Some(*result?);
                l.phase = Phase::Ready;
            }
            Phase::Ready => {
                let prepared = l
                    .ready
                    .as_ref()
                    .ok_or("missing retained region admission")?;
                if self.visibility_sources.len() + prepared.views.len() > 4096
                    || self.npc_sources.len() + prepared.npcs.len() > 4096
                    || self.binding_notices.len() + prepared.input.bindings.len() > 4096
                {
                    return Err("NPC source delivery capacity".into());
                }
                let mut bytes = self.source_bytes.values().sum::<usize>();
                let mut count = self.sources.len();
                for (&id, &size) in &prepared.source_bytes {
                    bytes = bytes - self.source_bytes.get(&id).copied().unwrap_or(0) + size;
                    count += usize::from(!self.sources.contains_key(&id));
                }
                if bytes > 64 * 1024 * 1024 || count > 4096 {
                    return Err("region source-cache capacity".into());
                }
                let ready = l.ready.take().ok_or("missing retained region admission")?;
                let binding_sources = ready
                    .input
                    .bindings
                    .iter()
                    .map(|binding| (binding.entity, binding.kind))
                    .collect();
                match worker
                    .input()
                    .try_admit_resident_region(l.request.token, ready.input)
                {
                    Ok(()) => {
                        l.views = ready.views;
                        l.npcs = ready.npcs;
                        l.bindings = binding_sources;
                        l.sources = ready.sources;
                        l.source_bytes = ready.source_bytes;
                        l.phase = Phase::Submitted;
                    }
                    Err(input) => {
                        l.ready = Some(cold::Finished {
                            views: ready.views,
                            npcs: ready.npcs,
                            input: *input,
                            sources: ready.sources,
                            source_bytes: ready.source_bytes,
                        })
                    }
                }
            }
            Phase::Submitted => {}
        }
        Ok(())
    }
}
fn limits() -> InventoryLoadLimits {
    InventoryLoadLimits {
        max_items: 4096,
        max_depth: 64,
        max_total_bytes: 64 * 1024 * 1024,
    }
}
#[cfg(test)]
mod tests;
