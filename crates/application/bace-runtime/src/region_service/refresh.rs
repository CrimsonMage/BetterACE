//! One retained, bounded off-thread refresh of occupied authored regions.
use super::*;
use std::collections::BTreeSet;

pub(super) fn affected(
    region: &PreparedRegionActivation,
    changed: &BTreeSet<bace_storage_codec::PackKey>,
    global_changed: bool,
) -> bool {
    let global = global_changed
        && region.content.templates.values().any(|template| {
            crate::generator_preparation::is_creature_template(template.weenie_type)
                || !template.properties.generators.is_empty()
                || !template.properties.create_list.is_empty()
        });
    selected(
        changed,
        region.fence.landblock,
        region.content.templates.keys().copied(),
        region
            .content
            .instances
            .iter()
            .map(|instance| instance.source.guid),
        region
            .content
            .encounters
            .iter()
            .map(|encounter| encounter.source.id),
        global,
    )
}

pub(super) fn global_changed(changed: &BTreeSet<bace_storage_codec::PackKey>) -> bool {
    changed.iter().any(|key| {
        matches!(key.namespace, 16..=45 | 46 | 47 | 50 | 52) && !matches!(key.namespace, 17 | 20)
    })
}

fn selected(
    changed: &BTreeSet<bace_storage_codec::PackKey>,
    block: u16,
    templates: impl Iterator<Item = u32>,
    instances: impl Iterator<Item = u32>,
    encounters: impl Iterator<Item = u32>,
    global: bool,
) -> bool {
    use bace_storage_codec::PackKey;
    global
        || changed.contains(&PackKey {
            namespace: 2,
            id: u64::from(block),
        })
        || templates.into_iter().any(|id| {
            changed.contains(&PackKey {
                namespace: 1,
                id: u64::from(id),
            })
        })
        || instances.into_iter().any(|id| {
            changed.contains(&PackKey {
                namespace: 20,
                id: u64::from(id),
            }) || changed.contains(&PackKey {
                namespace: 3,
                id: u64::from(id),
            })
        })
        || encounters.into_iter().any(|id| {
            changed.contains(&PackKey {
                namespace: 17,
                id: u64::from(id),
            })
        })
}

#[derive(Clone)]
pub(super) struct Head {
    epoch: u64,
    revision: u64,
    manifest: [u8; 32],
    ids: BTreeSet<u32>,
}
pub(super) struct Prepared {
    block: u16,
    expected: Head,
    next: Head,
    region: Arc<PreparedRegionActivation>,
    input: Option<PreparedResidentRegion>,
    views: Vec<crate::visibility_assets::PreparedVisibilityObject>,
    removed: Vec<EntityId>,
    correlation: u64,
    submitted: bool,
    retry_at: Option<std::time::Instant>,
}
struct AssetJob {
    old: Arc<PreparedRegionActivation>,
    head: Head,
    request: RegionActivationRequest,
    options: crate::generator_preparation::GeneratorPreparationOptions,
}
struct AssetCompletion {
    block: u16,
    manifest: [u8; 32],
    result: Result<Option<Prepared>, String>,
}
pub(super) struct Worker {
    jobs: std::sync::mpsc::SyncSender<AssetJob>,
    results: std::sync::mpsc::Receiver<AssetCompletion>,
    thread: std::thread::JoinHandle<()>,
}
impl Worker {
    pub(super) fn start(
        manifest: crate::region_activation::RegionAssetManifest,
    ) -> Result<Self, String> {
        let (jobs, inbox) = std::sync::mpsc::sync_channel::<AssetJob>(1);
        let (outbox, results) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("bace-resident-refresh".into())
            .spawn(move || {
                let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest);
                while let Ok(job) = inbox.recv() {
                    let block = job.request.landblock;
                    let manifest = job.request.content_hash;
                    let result = assets.as_mut().map_err(|e| e.clone()).and_then(|assets| {
                        prepare(job.old, job.head, job.request, assets, job.options)
                    });
                    if outbox
                        .send(AssetCompletion {
                            block,
                            manifest,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            jobs,
            results,
            thread,
        })
    }
    fn try_submit(
        &self,
        job: AssetJob,
    ) -> Result<(), Box<std::sync::mpsc::TrySendError<AssetJob>>> {
        self.jobs.try_send(job).map_err(Box::new)
    }
    fn try_recv(&self) -> Result<AssetCompletion, std::sync::mpsc::TryRecvError> {
        self.results.try_recv()
    }
    pub(super) fn shutdown(self) -> Result<usize, String> {
        drop(self.jobs);
        let completions = self.results.into_iter().count();
        self.thread
            .join()
            .map_err(|_| "resident refresh worker panicked".to_string())?;
        Ok(completions)
    }
}
#[derive(Default)]
pub(super) struct RefreshState {
    queued: VecDeque<u16>,
    scheduled: BTreeSet<u16>,
    heads: BTreeMap<u16, Head>,
    job: Option<(u16, [u8; 32])>,
    current: Option<Prepared>,
    unexpected: Option<AssetCompletion>,
    failed_job: Option<AssetJob>,
}
impl RefreshState {
    pub(super) fn accepted_targets(
        &mut self,
        blocked: &mut BTreeMap<u16, String>,
        blocks: BTreeSet<u16>,
        capacity: usize,
    ) {
        blocked.retain(|block, reason| {
            !blocks.contains(block) || !reason.starts_with("resident refresh")
        });
        self.schedule(blocks.into_iter(), capacity);
    }
    pub(super) fn has_pending(&self) -> bool {
        self.job.is_some()
            || self.current.is_some()
            || !self.queued.is_empty()
            || self.unexpected.is_some()
            || self.failed_job.is_some()
    }
    pub(super) fn submitted(&self) -> bool {
        self.current.as_ref().is_some_and(|c| c.submitted)
    }
    pub(super) fn current_for(&self, block: u16) -> bool {
        self.current.as_ref().is_some_and(|c| c.block == block)
    }
    pub(super) fn schedule(&mut self, blocks: impl Iterator<Item = u16>, capacity: usize) {
        for block in blocks {
            if self.scheduled.len() < capacity && self.scheduled.insert(block) {
                self.queued.push_back(block);
            }
        }
    }
    pub(super) fn cold_admitted(&mut self, region: &PreparedRegionActivation) {
        let block = region.fence.landblock;
        self.heads.insert(
            block,
            Head {
                epoch: region.fence.activation_epoch,
                revision: region.fence.content_generation,
                manifest: region.source_manifest,
                ids: authored_ids(region),
            },
        );
    }
    pub(super) fn unloaded(&mut self, block: u16) {
        self.heads.remove(&block);
        self.scheduled.remove(&block);
        self.queued.retain(|queued| *queued != block);
        if self
            .failed_job
            .as_ref()
            .is_some_and(|job| job.request.landblock == block)
        {
            self.failed_job = None;
        }
        if self
            .current
            .as_ref()
            .is_some_and(|current| current.block == block && !current.submitted)
        {
            self.current = None;
        }
    }
    pub(super) fn retry(&mut self, block: u16, capacity: usize) {
        self.scheduled.remove(&block);
        self.schedule(std::iter::once(block), capacity);
    }
}
fn authored_ids(region: &PreparedRegionActivation) -> BTreeSet<u32> {
    region
        .content
        .instances
        .iter()
        .filter(|instance| !instance.source.is_link_child)
        .map(|instance| instance.source.guid)
        .collect()
}
fn retry_due(
    reason: &str,
    deadline: Option<std::time::Instant>,
    now: std::time::Instant,
    unloading: bool,
) -> bool {
    !unloading
        && reason.starts_with("resident refresh rejected:")
        && deadline.is_some_and(|deadline| now >= deadline)
}

impl RegionService {
    pub(super) fn poll_refresh(&mut self, worker: &SimulationWorker) -> Result<(), String> {
        if self.refresh.unexpected.is_some() {
            return Err("unmatched resident refresh completion retained".into());
        }
        if self.refresh.failed_job.is_some() {
            return Err("resident refresh worker disconnected; prepared request retained".into());
        }
        if let Some((block, manifest)) = self.refresh.job {
            let result = match self.refresh_worker.try_recv() {
                Ok(completion) if completion.block == block && completion.manifest == manifest => {
                    completion.result
                }
                Ok(completion) => {
                    self.refresh.unexpected = Some(completion);
                    return Err(
                        "resident refresh worker correlation mismatch; completion retained".into(),
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("resident refresh worker disconnected with retained job".into());
                }
            };
            self.refresh.job = None;
            match result {
                Ok(Some(prepared)) => self.refresh.current = Some(prepared),
                Ok(None) => {
                    self.refresh.scheduled.remove(&block);
                    if self.config.content_hash != manifest {
                        self.refresh
                            .schedule(std::iter::once(block), self.config.capacity);
                    }
                }
                Err(error) => {
                    self.refresh.scheduled.remove(&block);
                    self.blocked
                        .insert(block, format!("resident refresh preparation: {error}"));
                    return Ok(());
                }
            }
        }
        if let Some(current) = self.refresh.current.as_mut() {
            if let Some(reason) = self.blocked.get(&current.block) {
                if retry_due(
                    reason,
                    current.retry_at,
                    std::time::Instant::now(),
                    self.unload.is_some(),
                ) {
                    self.blocked.remove(&current.block);
                    current.retry_at = None;
                } else {
                    return Ok(());
                }
            }
            if current.submitted || self.loading.is_some() || self.unload.is_some() {
                return Ok(());
            }
            if self.refresh.heads.get(&current.block).is_none_or(|head| {
                head.epoch != current.expected.epoch
                    || head.revision != current.expected.revision
                    || head.manifest != current.expected.manifest
            }) || self.config.content_hash != current.next.manifest
            {
                let block = current.block;
                self.refresh.current = None;
                self.refresh.scheduled.remove(&block);
                self.refresh
                    .schedule(std::iter::once(block), self.config.capacity);
                return Ok(());
            }
            let input = current.input.take().expect("retained refresh input");
            if self
                .visibility_retirements
                .len()
                .saturating_add(current.removed.len())
                > 4096
                || self
                    .visibility_sources
                    .len()
                    .saturating_add(current.views.len())
                    > 4096
            {
                current.input = Some(input);
                return Ok(());
            }
            match worker
                .input()
                .try_admit_resident_region(current.correlation, input)
            {
                Ok(()) => current.submitted = true,
                Err(input) => current.input = Some(*input),
            }
            return Ok(());
        }
        if self.refresh.job.is_some() || self.loading.is_some() || self.unload.is_some() {
            return Ok(());
        }
        let Some(block) = self.refresh.queued.pop_front() else {
            return Ok(());
        };
        let Some(old) = self.active.get(&block).cloned() else {
            self.refresh.scheduled.remove(&block);
            return Ok(());
        };
        let Some(head) = self.refresh.heads.get(&block).cloned() else {
            self.refresh.scheduled.remove(&block);
            return Ok(());
        };
        let request = RegionActivationRequest {
            token: self
                .next
                .checked_add(1)
                .ok_or("region refresh token exhausted")?,
            activation_epoch: head.epoch,
            landblock: block,
            generation: self.config.generation.clone(),
            creature_policy: Some(self.config.creature_policy),
            treasure_assets: Some(self.config.treasure_assets.clone()),
            content_hash: self.config.content_hash,
            aetheria_drop_rate: self.config.aetheria_drop_rate,
        };
        self.next = request.token;
        let options = self.config.options;
        let manifest = request.content_hash;
        match self.refresh_worker.try_submit(AssetJob {
            old,
            head,
            request,
            options,
        }) {
            Ok(()) => self.refresh.job = Some((block, manifest)),
            Err(error) => match *error {
                std::sync::mpsc::TrySendError::Full(_) => self.refresh.queued.push_front(block),
                std::sync::mpsc::TrySendError::Disconnected(job) => {
                    self.refresh.failed_job = Some(job);
                    return Err(
                        "resident refresh worker disconnected; prepared request retained".into(),
                    );
                }
            },
        }
        Ok(())
    }
    pub(super) fn poll_refresh_admission(
        &mut self,
        worker: &SimulationWorker,
    ) -> Result<(), String> {
        let Some(current) = self.refresh.current.as_mut().filter(|c| c.submitted) else {
            return Ok(());
        };
        let Ok(outcome) = worker.region_admission_outcomes().try_recv() else {
            return Ok(());
        };
        if outcome.correlation != current.correlation {
            self.unexpected = Some(outcome);
            return Err("unexpected region refresh correlation retained".into());
        }
        match outcome.result {
            Ok(()) => {
                let done = self.refresh.current.take().expect("matched refresh");
                self.active.insert(done.block, done.region);
                self.refresh.heads.insert(done.block, done.next);
                self.refresh.scheduled.remove(&done.block);
                self.visibility_sources
                    .extend(done.views.into_iter().map(|view| (outcome.tick, view)));
                self.visibility_retirements
                    .extend(done.removed.into_iter().map(|id| (id, outcome.tick)));
                if self.config.content_hash != self.refresh.heads[&done.block].manifest {
                    self.refresh
                        .schedule(std::iter::once(done.block), self.config.capacity);
                }
            }
            Err((error, input)) => {
                let block = current.block;
                if matches!(
                    error,
                    bace_simulation::GeneratorServiceError::Busy
                        | bace_simulation::GeneratorServiceError::Capacity
                ) {
                    current.input = Some(*input);
                    current.submitted = false;
                    current.retry_at =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
                } else {
                    self.refresh.current = None;
                    self.refresh.scheduled.remove(&block);
                }
                self.blocked
                    .insert(block, format!("resident refresh rejected: {error:?}"));
            }
        }
        Ok(())
    }
}

fn prepare(
    old: Arc<PreparedRegionActivation>,
    head: Head,
    request: RegionActivationRequest,
    assets: &mut crate::region_activation::VerifiedRegionAssets,
    options: crate::generator_preparation::GeneratorPreparationOptions,
) -> Result<Option<Prepared>, String> {
    if unaffected(&old, &request.generation)? {
        return Ok(None);
    }
    let mut region = assets.prepare(&request)?;
    let next_ids = authored_ids(&region);
    let remove_roots: Vec<_> = head
        .ids
        .difference(&next_ids)
        .copied()
        .map(EntityId)
        .collect();
    if old
        .content
        .encounters
        .iter()
        .map(|e| e.source.id)
        .collect::<BTreeSet<_>>()
        != region
            .content
            .encounters
            .iter()
            .map(|e| e.source.id)
            .collect::<BTreeSet<_>>()
    {
        return Err("live encounter-row changes require durable GUID reconciliation".into());
    }
    let all_instances = std::mem::take(&mut region.content.instances);
    let all_encounters = std::mem::take(&mut region.content.encounters);
    region.content.instances = all_instances
        .iter()
        .filter(|instance| {
            !instance.source.is_link_child && !head.ids.contains(&instance.source.guid)
        })
        .cloned()
        .collect();
    let added_templates: BTreeSet<_> = region
        .content
        .instances
        .iter()
        .map(|instance| instance.template.weenie_id)
        .collect();
    let full_templates = std::mem::take(&mut region.content.templates);
    region.content.templates = full_templates
        .iter()
        .filter(|(id, _)| added_templates.contains(id))
        .map(|(&id, value)| (id, value.clone()))
        .collect();
    let full_creatures = std::mem::take(&mut region.creatures);
    region.creatures = full_creatures
        .iter()
        .filter(|(id, _)| added_templates.contains(id))
        .map(|(&id, value)| (id, value.clone()))
        .collect();
    let mut batch = region.prepare_generator_admission(&BTreeMap::new(), options)?;
    region.content.templates = full_templates;
    region.creatures = full_creatures;
    let additions = std::mem::replace(&mut region.content.instances, all_instances.clone());
    let template_keys = old
        .content
        .templates
        .keys()
        .map(|id| bace_storage_codec::PackKey {
            namespace: 1,
            id: u64::from(*id),
        })
        .collect();
    let templates_changed = !same_keys(&old.generation, &request.generation, template_keys)?;
    for instance in all_instances.iter().filter(|instance| {
        !instance.source.is_link_child && head.ids.contains(&instance.source.guid)
    }) {
        let template = instance.template.as_ref();
        if template.properties.generators.is_empty() && template.weenie_type != 12 {
            continue;
        }
        let source = &instance.source;
        if !templates_changed
            && same_keys(
                &old.generation,
                &request.generation,
                BTreeSet::from([
                    bace_storage_codec::PackKey {
                        namespace: 20,
                        id: u64::from(source.guid),
                    },
                    bace_storage_codec::PackKey {
                        namespace: 3,
                        id: u64::from(source.guid),
                    },
                ]),
            )?
        {
            continue;
        }
        let location = bace_gameplay_api::GeneratorLocation {
            cell: source.obj_cell_id,
            origin: [source.origin_x, source.origin_y, source.origin_z],
            rotation: [
                source.angles_x,
                source.angles_y,
                source.angles_z,
                source.angles_w,
            ],
        };
        let links =
            crate::generator_preparation::region_generator_links(&region.content, source.guid)?;
        batch
            .admission
            .definitions
            .push(crate::generator_preparation::prepare_generator(
                template,
                crate::region_activation::region_generator_identity(
                    EntityId(source.guid),
                    head.epoch,
                    region.fence.content_generation,
                ),
                location,
                &links,
                options,
            )?);
    }
    region.content.instances = additions;
    if !batch.admission.definitions.is_empty() {
        for (&id, template) in &region.content.templates {
            if !template.properties.generators.is_empty() || template.weenie_type == 12 {
                batch.admission.templates.push((
                    id,
                    crate::generator_preparation::prepare_generator(
                        template,
                        crate::region_activation::region_generator_identity(
                            EntityId(id),
                            1,
                            region.fence.content_generation,
                        ),
                        bace_gameplay_api::GeneratorLocation {
                            cell: 0x0001_0001,
                            origin: [0.0; 3],
                            rotation: [0.0, 0.0, 0.0, 1.0],
                        },
                        &[],
                        options,
                    )?,
                ));
            }
        }
        for (&id, creature) in &region.creatures {
            batch
                .admission
                .creatures
                .push((id, creature.as_ref().map_err(Clone::clone)?.clone()));
        }
    }
    if !batch.held.is_empty()
        || batch
            .admission
            .roots
            .iter()
            .any(|root| root.creature.is_some() || root.script.is_some() || root.loadout.is_some())
        || !batch.admission.containers.is_empty()
    {
        return Err(
            "live refresh has complex authored roots; retained until dedicated migration".into(),
        );
    }
    let admitted = batch
        .admission
        .roots
        .iter()
        .map(|root| root.entity)
        .collect();
    let views = assets.prepare_region_visibility(
        &region,
        &BTreeMap::new(),
        &admitted,
        &bace_simulation::PreparedWorldRegionItems::default(),
        &BTreeMap::new(),
    )?;
    region.content.instances = all_instances;
    region.content.encounters = all_encounters;
    let keep_alive = region
        .content
        .instances
        .iter()
        .filter(|instance| !instance.source.is_link_child && instance.template.weenie_id == 80007)
        .count();
    let next = Head {
        epoch: head.epoch,
        revision: region.fence.content_generation,
        manifest: region.source_manifest,
        ids: next_ids,
    };
    let input = PreparedResidentRegion {
        source_manifest: region.source_manifest,
        refresh: Some(bace_simulation::ResidentRegionRefresh {
            expected_epoch: head.epoch,
            expected_revision: head.revision,
            expected_manifest: head.manifest,
            remove_roots: remove_roots.clone(),
        }),
        visibility: region.visibility.clone(),
        region: batch.admission,
        items: bace_simulation::PreparedWorldRegionItems::default(),
        bindings: vec![],
        dungeon: false,
        keep_alive: u32::try_from(keep_alive).map_err(|_| "region keep-alive count overflow")?,
    };
    Ok(Some(Prepared {
        block: request.landblock,
        expected: head,
        next,
        region: Arc::new(region),
        input: Some(input),
        views,
        removed: remove_roots,
        correlation: request.token,
        submitted: false,
        retry_at: None,
    }))
}

/// The common plain-object case avoids opening DATs or decoding an entire
/// regional closure when neither its index, rows, links nor templates changed.
/// Generator and creature regions remain conservative because global treasure
/// rows can change their future preparation without touching a local template.
fn unaffected(
    old: &PreparedRegionActivation,
    next: &bace_storage_codec::PackGeneration,
) -> Result<bool, String> {
    use bace_storage_codec::PackKey;
    if old.content.templates.values().any(|template| {
        crate::generator_preparation::is_creature_template(template.weenie_type)
            || !template.properties.generators.is_empty()
            || !template.properties.create_list.is_empty()
    }) || old
        .content
        .instances
        .iter()
        .any(|instance| !instance.links.is_empty())
    {
        return Ok(false);
    }
    let mut keys = BTreeSet::new();
    keys.insert(PackKey {
        namespace: 2,
        id: u64::from(old.fence.landblock),
    });
    for instance in &old.content.instances {
        keys.insert(PackKey {
            namespace: 20,
            id: u64::from(instance.source.guid),
        });
        keys.insert(PackKey {
            namespace: 3,
            id: u64::from(instance.source.guid),
        });
    }
    for encounter in &old.content.encounters {
        keys.insert(PackKey {
            namespace: 17,
            id: u64::from(encounter.source.id),
        });
    }
    for &template in old.content.templates.keys() {
        keys.insert(PackKey {
            namespace: 1,
            id: u64::from(template),
        });
    }
    same_keys(&old.generation, next, keys)
}

fn same_keys(
    old: &bace_storage_codec::PackGeneration,
    next: &bace_storage_codec::PackGeneration,
    keys: BTreeSet<bace_storage_codec::PackKey>,
) -> Result<bool, String> {
    use bace_storage_codec::PackLookup;
    for key in keys {
        let before = old.lookup(key).map_err(|e| e.to_string())?;
        let after = next.lookup(key).map_err(|e| e.to_string())?;
        let same = match (before, after) {
            (PackLookup::Record(a), PackLookup::Record(b)) => {
                a.schema() == b.schema() && a.bytes() == b.bytes()
            }
            (PackLookup::Missing, PackLookup::Missing)
            | (PackLookup::Tombstone, PackLookup::Tombstone) => true,
            _ => false,
        };
        if !same {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{Head, RefreshState, retry_due, same_keys, selected};
    use bace_storage_codec::{PackKey, PackLimits, PackManifest, PackRecord};
    use std::collections::BTreeSet;

    #[test]
    fn unrelated_delta_skips_region_while_index_change_selects_it() {
        let dir = tempfile::tempdir().unwrap();
        let limits = PackLimits::default();
        let local = PackKey {
            namespace: 2,
            id: 0x8602,
        };
        let base = bace_storage_codec::compile_pack(
            dir.path(),
            [Ok(PackRecord {
                key: local,
                schema: 1,
                value: Some(vec![1]),
            })],
            limits,
        )
        .unwrap();
        let old_manifest = PackManifest {
            version: 1,
            generation: 1,
            base,
            deltas: vec![],
        };
        let old = old_manifest.open(dir.path(), limits).unwrap();
        let unrelated = bace_storage_codec::compile_pack(
            dir.path(),
            [Ok(PackRecord {
                key: PackKey {
                    namespace: 1,
                    id: 900000,
                },
                schema: 1,
                value: Some(vec![2]),
            })],
            limits,
        )
        .unwrap();
        let new_manifest = PackManifest {
            version: 1,
            generation: 2,
            base: old_manifest.base.clone(),
            deltas: vec![unrelated],
        };
        let new = new_manifest.open(dir.path(), limits).unwrap();
        let selected = BTreeSet::from([local]);
        assert!(same_keys(&old, &new, selected.clone()).unwrap());
        let changed = bace_storage_codec::compile_pack(
            dir.path(),
            [Ok(PackRecord {
                key: local,
                schema: 1,
                value: Some(vec![3]),
            })],
            limits,
        )
        .unwrap();
        let changed_manifest = PackManifest {
            version: 1,
            generation: 3,
            base: old_manifest.base,
            deltas: vec![changed],
        };
        let changed = changed_manifest.open(dir.path(), limits).unwrap();
        assert!(!same_keys(&old, &changed, selected).unwrap());
    }
    #[test]
    fn weenie_dependency_and_landblock_index_select_only_affected_residents() {
        let target = |key| {
            selected(
                &BTreeSet::from([key]),
                0x8602,
                [30997].into_iter(),
                [0x7001].into_iter(),
                [44].into_iter(),
                false,
            )
        };
        assert!(target(PackKey {
            namespace: 1,
            id: 30997
        }));
        assert!(target(PackKey {
            namespace: 2,
            id: 0x8602
        }));
        assert!(target(PackKey {
            namespace: 20,
            id: 0x7001
        }));
        assert!(!target(PackKey {
            namespace: 1,
            id: 30998
        }));
        assert!(!target(PackKey {
            namespace: 2,
            id: 0x8603
        }));
        assert!(!target(PackKey {
            namespace: 39,
            id: 9
        }));
    }
    #[test]
    fn unrelated_acceptance_keeps_failed_resident_head_and_diagnostic() {
        let mut refresh = RefreshState::default();
        refresh.heads.insert(
            0x8602,
            Head {
                epoch: 7,
                revision: 11,
                manifest: [1; 32],
                ids: BTreeSet::from([100]),
            },
        );
        let mut blocked = std::collections::BTreeMap::from([(
            0x8602,
            "resident refresh preparation: missing asset".to_string(),
        )]);
        refresh.accepted_targets(&mut blocked, BTreeSet::from([0x8603]), 8);
        assert!(blocked.contains_key(&0x8602));
        assert_eq!(refresh.heads[&0x8602].revision, 11);
        assert!(!refresh.queued.contains(&0x8602));
        refresh.accepted_targets(&mut blocked, BTreeSet::from([0x8602]), 8);
        assert!(!blocked.contains_key(&0x8602));
        assert_eq!(refresh.heads[&0x8602].manifest, [1; 32]);
        assert!(refresh.queued.contains(&0x8602));
    }
    #[test]
    fn busy_refresh_has_one_second_retry_and_unload_never_clears_its_hold() {
        let start = std::time::Instant::now();
        let due = start + std::time::Duration::from_secs(1);
        let reason = "resident refresh rejected: Busy";
        assert!(!retry_due(reason, Some(due), start, false));
        assert!(!retry_due(
            reason,
            Some(due),
            due - std::time::Duration::from_millis(1),
            false
        ));
        assert!(retry_due(reason, Some(due), due, false));
        assert!(retry_due(
            "resident refresh rejected: Capacity",
            Some(due),
            due,
            false
        ));
        assert!(!retry_due(reason, Some(due), due, true));
        assert!(!retry_due(
            "uncertain region unload: database",
            Some(due),
            due,
            false
        ));
        assert!(!retry_due(reason, None, due, false));
    }
}
