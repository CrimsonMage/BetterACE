//! A single bounded preparation worker, separate from simulation and save I/O.
use super::*;
use std::{sync::mpsc, thread};
pub(super) enum Job {
    PinNpc {
        prepared: Arc<PreparedRegionActivation>,
        request: RegionActivationRequest,
        pin: Arc<crate::npc_region::NpcRegionRequest>,
        directory: Arc<std::path::PathBuf>,
    },
    Gear {
        prepared: Arc<PreparedRegionActivation>,
        encounters: BTreeMap<u32, EntityId>,
        random: Arc<bace_random::RandomRoot>,
        world_epoch: u64,
        drop_plain_wield: bool,
    },
    Finish {
        prepared: Arc<PreparedRegionActivation>,
        encounters: BTreeMap<u32, EntityId>,
        gear: Arc<equipment::RegionEquipment>,
        ids: Vec<EntityId>,
        snapshots: Arc<Vec<LocatedSnapshot>>,
        clock: RegionClock,
        generation: Arc<bace_storage_codec::PackGeneration>,
        options: crate::generator_preparation::GeneratorPreparationOptions,
        suppressed: Arc<std::collections::BTreeSet<EntityId>>,
        content_hash: [u8; 32],
    },
}
pub(super) enum Completion {
    PinNpc(Result<Box<crate::npc_region::PreparedNpcRegionSource>, String>),
    Gear(Result<equipment::RegionEquipment, String>),
    Finish(Result<Box<Finished>, String>),
}
pub(super) struct Finished {
    pub views: Vec<crate::visibility_assets::PreparedVisibilityObject>,
    pub npcs: Vec<crate::npc_sources::PreparedNpcRegistration>,
    pub input: PreparedResidentRegion,
    pub sources: BTreeMap<u32, crate::region_unload_saves::RegionItemSource>,
    pub source_bytes: BTreeMap<u32, usize>,
}
pub(super) struct Worker {
    jobs: mpsc::SyncSender<Job>,
    results: mpsc::Receiver<Completion>,
    thread: thread::JoinHandle<()>,
}
impl Worker {
    pub fn start(manifest: crate::region_activation::RegionAssetManifest) -> Result<Self, String> {
        let (jobs, inbox) = mpsc::sync_channel::<Job>(1);
        let (outbox, results) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("bace-region-restore".into())
            .spawn(move || {
                let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest);
                let mut spell_table = None;
                while let Ok(job) = inbox.recv() {
                    let completed = match job {
                        Job::PinNpc {
                            prepared,
                            request,
                            pin,
                            directory,
                        } => Completion::PinNpc(assets.as_mut().map_err(|e| e.clone()).and_then(
                            |assets| {
                                assets
                                    .prepare_pinned_npc_region(
                                        &prepared, &request, &pin, &directory,
                                    )
                                    .map(Box::new)
                            },
                        )),
                        Job::Gear {
                            prepared,
                            encounters,
                            random,
                            world_epoch,
                            drop_plain_wield,
                        } => Completion::Gear(equipment::materialize(
                            &prepared,
                            &encounters,
                            &random,
                            world_epoch,
                            drop_plain_wield,
                        )),
                        Job::Finish {
                            prepared,
                            encounters,
                            gear,
                            ids,
                            snapshots,
                            clock,
                            generation,
                            options,
                            suppressed,
                            content_hash,
                        } => Completion::Finish((|| {
                            if prepared.source_manifest != content_hash
                                || prepared.fence.content_generation != generation.revision()
                            {
                                return Err("region finish content fence mismatch".into());
                            }
                            let assets = assets.as_mut().map_err(|e| e.clone())?;
                            if spell_table.is_none() {
                                spell_table = Some(assets.prepare_world_spell_table()?);
                            }
                            let table = spell_table.as_ref().expect("prepared spell table").clone();
                            let spells =
                                crate::generator_spell_assets::prepare_generator_spell_rows(
                                    &generation,
                                    table.clone(),
                                    &equipment::spell_templates(&gear),
                                )?;
                            let mut loadouts = equipment::bind(&prepared, &gear, &ids, &spells)?;
                            loadouts.retain(|id, _| !suppressed.contains(id));
                            let (items, mut sources) = world_items::prepare(
                                &snapshots,
                                &prepared,
                                assets,
                                &table,
                                world_items::WorldItemRestoreClock {
                                    epoch: prepared.fence.activation_epoch,
                                    unix_seconds: clock.unix_seconds,
                                    tick: clock.tick,
                                },
                            )?;
                            for (actor, loadout) in &loadouts {
                                equipment::retain_sources(
                                    *actor,
                                    loadout,
                                    &gear,
                                    &ids,
                                    prepared
                                        .npc_recovery
                                        .get(actor)
                                        .map_or(prepared.fence.content_generation, |pin| {
                                            pin.prepared.fence.content_generation
                                        }),
                                    &mut sources,
                                )?;
                            }
                            for (actor, loadout) in &mut loadouts {
                                let source = prepared
                                    .content
                                    .instances
                                    .iter()
                                    .find(|i| i.source.guid == actor.0)
                                    .map(|i| i.template.as_ref())
                                    .or_else(|| {
                                        prepared
                                            .content
                                            .encounters
                                            .iter()
                                            .find(|e| encounters.get(&e.source.id) == Some(actor))
                                            .map(|e| e.template.as_ref())
                                    })
                                    .ok_or("NPC loadout source locator missing")?;
                                let equipment = loadout
                                    .profile
                                    .equipment
                                    .iter()
                                    .map(|stamp| {
                                        let item = sources
                                            .get(&stamp.entity)
                                            .ok_or("NPC equipped source absent")?;
                                        if item.item.entity.mutation_revision != stamp.revision {
                                            return Err("NPC equipped source revision mismatch");
                                        }
                                        Ok(bace_combat::preparation::PhysicalEquipmentSource {
                                            entity: stamp.entity,
                                            revision: stamp.revision,
                                            location: stamp.location,
                                            weenie: Arc::new(item.item.entity.state.clone()),
                                        })
                                    })
                                    .collect::<Result<Vec<_>, &str>>()?;
                                assets.prepare_npc_loadout_motions(
                                    source,
                                    loadout,
                                    &equipment,
                                    prepared
                                        .npc_recovery
                                        .get(actor)
                                        .map_or(prepared.generation.as_ref(), |pin| {
                                            pin.prepared.generation.as_ref()
                                        }),
                                )?;
                            }
                            for (actor, loadout) in &mut loadouts {
                                if let Some(pin) = prepared.npc_recovery.get(actor) {
                                    crate::npc_region::restore_loadout(pin, loadout, &sources)?;
                                }
                            }
                            let mut batch = prepared.prepare_generator_admission_with_loadouts(
                                &encounters,
                                options,
                                &loadouts,
                            )?;
                            batch
                                .held
                                .retain(|(id, _)| !suppressed.contains(&EntityId(*id)));
                            batch
                                .admission
                                .roots
                                .retain(|r| !suppressed.contains(&r.entity));
                            batch
                                .admission
                                .containers
                                .retain(|c| !suppressed.contains(&c.id));
                            batch
                                .admission
                                .definitions
                                .retain(|d| !suppressed.contains(&d.identity.entity));
                            let admitted = batch.admission.roots.iter().map(|r| r.entity).collect();
                            let mut npcs = crate::npc_sources::prepare_region(
                                &prepared,
                                &encounters,
                                &admitted,
                                prepared.source_manifest,
                            )?;
                            for registration in &mut npcs {
                                let root = batch
                                    .admission
                                    .roots
                                    .iter_mut()
                                    .find(|r| r.entity == registration.actor)
                                    .ok_or("NPC source script root absent")?;
                                root.script = Some(registration.script_source());
                                registration.admitted = true;
                            }
                            if !batch.held.is_empty() {
                                return Err(format!("region has held roots: {:?}", batch.held));
                            }
                            let bindings = crate::region_activation::prepare_binding_objects(
                                &prepared,
                                &batch.admission,
                            )?;
                            let views = assets.prepare_region_visibility(
                                &prepared,
                                &encounters,
                                &admitted,
                                &items,
                                &sources,
                            )?;
                            let keep_alive = prepared
                                .content
                                .instances
                                .iter()
                                .filter(|i| {
                                    !i.source.is_link_child && i.template.weenie_id == 80007
                                })
                                .count();
                            let dungeon =
                                prepared.geometry.cell_ids().all(|id| id & 0xffff >= 0x100);
                            let source_bytes = sources
                                .iter()
                                .map(|(&id, s)| {
                                    world_items::source_size(s).map(|bytes| (id, bytes))
                                })
                                .collect::<Result<BTreeMap<_, _>, _>>()?;
                            if source_bytes.values().sum::<usize>() > 64 * 1024 * 1024 {
                                return Err("region source metadata exceeds byte budget".into());
                            }
                            Ok(Box::new(Finished {
                                views,
                                npcs,
                                input: PreparedResidentRegion {
                                    source_manifest: prepared.source_manifest,
                                    refresh: None,
                                    visibility: prepared.visibility.clone(),
                                    region: batch.admission,
                                    items,
                                    bindings,
                                    dungeon,
                                    keep_alive: u32::try_from(keep_alive)
                                        .map_err(|_| "keep-alive count overflow")?,
                                },
                                sources,
                                source_bytes,
                            }))
                        })()),
                    };
                    if outbox.send(completed).is_err() {
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
    pub fn submit(&self, job: Job) -> Result<(), Box<Job>> {
        self.jobs.try_send(job).map_err(|e| match e {
            mpsc::TrySendError::Full(j) | mpsc::TrySendError::Disconnected(j) => Box::new(j),
        })
    }
    pub fn receive(&self) -> Option<Completion> {
        self.results.try_recv().ok()
    }
    pub fn shutdown(self) -> Result<Vec<Completion>, String> {
        drop(self.jobs);
        let result = self.results.into_iter().collect();
        self.thread
            .join()
            .map_err(|_| "region restore worker panicked")?;
        Ok(result)
    }
}
