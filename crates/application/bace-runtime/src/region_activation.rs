//! Bounded cold DAT/content join. Every completion retains its immutable pack
//! generation and activation fence; only the simulation owner can adopt it.
mod avatar;
mod avatar_action;
mod bindings;
pub(crate) use bindings::prepare_binding_objects;
mod crafting;
mod deaths;
mod physical;
mod recalls;
pub use recalls::PreparedRecallMotion;
mod creation;
mod generators;
mod magic_procs;
mod materials;
mod npc_recovery;
mod projectile_visibility;
mod restoration;
mod startup;
mod visibility;
use bace_dat::{
    Animation, CollisionSetup, DatArchive, EnvCell, Environment, GraphicsObject, Landblock,
    LandblockInfo, MotionTable, RegionLand,
};
use bace_physics::{CollisionShape, GeometryRegion};
use bace_storage_codec::PackGeneration;
pub use generators::PreparedGeneratorActivation;
pub(crate) use generators::identity as region_generator_identity;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

#[derive(Clone)]
pub struct RegionAssetManifest {
    pub portal: PathBuf,
    pub cell: PathBuf,
    pub portal_sha256: String,
    pub cell_sha256: String,
}
#[derive(Clone)]
pub struct RegionActivationRequest {
    pub token: u64,
    pub activation_epoch: u64,
    pub landblock: u16,
    pub generation: Arc<PackGeneration>,
    pub creature_policy: Option<crate::generator_preparation::CreatureAdmissionPolicy>,
    pub treasure_assets: Option<Arc<bace_loot::TreasureAssets>>,
    pub content_hash: [u8; 32],
    pub aetheria_drop_rate: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionActivationFence {
    pub token: u64,
    pub activation_epoch: u64,
    pub landblock: u16,
    pub content_generation: u64,
}
#[derive(Clone)]
pub struct PreparedPhysicalTemplate {
    pub shape: Arc<CollisionShape>,
    /// Unsupported animation data is retained as an observable held admission.
    pub locomotion: Result<Arc<bace_motion::AnimatedLocomotion>, String>,
}
pub struct PreparedRegionActivation {
    pub source_manifest: [u8; 32],
    pub npc_recovery:
        BTreeMap<bace_types::EntityId, Arc<crate::npc_region::PreparedNpcRegionSource>>,
    /// Exact immutable generation used by this region's cold source closure.
    pub generation: Arc<PackGeneration>,
    pub visibility: Vec<bace_world::PreparedCellVisibility>,
    pub fence: RegionActivationFence,
    pub content: crate::world_content::PreparedRegion,
    pub catalog: crate::generator_catalog::PreparedGeneratorCatalog,
    pub item_spells: Result<crate::generator_spell_assets::PreparedGeneratorSpellAssets, String>,
    pub geometry: Arc<GeometryRegion>,
    pub physical: BTreeMap<u32, Result<PreparedPhysicalTemplate, String>>,
    pub creatures: BTreeMap<u32, Result<bace_simulation::GeneratedNpcTemplate, String>>,
}
pub struct RegionActivationCompletion {
    pub fence: RegionActivationFence,
    pub result: Result<PreparedRegionActivation, String>,
}
pub struct RegionActivationWorker {
    jobs: mpsc::SyncSender<RegionActivationRequest>,
    results: mpsc::Receiver<RegionActivationCompletion>,
    thread: thread::JoinHandle<()>,
}
impl RegionActivationWorker {
    pub fn start(manifest: RegionAssetManifest, capacity: usize) -> Result<Self, String> {
        if !(1..=4).contains(&capacity) {
            return Err("region activation capacity must be 1..4".into());
        }
        let (jobs, inbox) = mpsc::sync_channel::<RegionActivationRequest>(capacity);
        let (outbox, results) = mpsc::sync_channel(capacity);
        let thread = thread::Builder::new()
            .name("bace-region-admission".into())
            .spawn(move || {
                let mut assets = VerifiedRegionAssets::open(&manifest);
                while let Ok(request) = inbox.recv() {
                    let fence = RegionActivationFence {
                        token: request.token,
                        activation_epoch: request.activation_epoch,
                        landblock: request.landblock,
                        content_generation: request.generation.revision(),
                    };
                    let result = match &mut assets {
                        Ok(assets) => assets.prepare(&request),
                        Err(error) => Err(error.clone()),
                    };
                    if outbox
                        .send(RegionActivationCompletion { fence, result })
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
    pub fn try_submit(
        &self,
        request: RegionActivationRequest,
    ) -> Result<(), RegionActivationRequest> {
        self.jobs.try_send(request).map_err(|e| match e {
            mpsc::TrySendError::Full(v) | mpsc::TrySendError::Disconnected(v) => v,
        })
    }
    pub fn try_recv(&self) -> Result<RegionActivationCompletion, mpsc::TryRecvError> {
        self.results.try_recv()
    }
    pub fn shutdown(self) -> Result<Vec<RegionActivationCompletion>, String> {
        drop(self.jobs);
        let results = self.results.into_iter().collect();
        self.thread.join().map_err(|_| "region worker panicked")?;
        Ok(results)
    }
}
/// Owns verified immutable archives exclusively on the preparation thread.
pub struct VerifiedRegionAssets {
    portal: DatArchive,
    cell: DatArchive,
    land: RegionLand,
}
impl VerifiedRegionAssets {
    pub fn open(manifest: &RegionAssetManifest) -> Result<Self, String> {
        for (path, hash) in [
            (&manifest.portal, &manifest.portal_sha256),
            (&manifest.cell, &manifest.cell_sha256),
        ] {
            if hash.len() != 64 || bace_dat::fingerprint(path).map_err(|e| e.to_string())? != *hash
            {
                return Err("unapproved region DAT fingerprint".into());
            }
        }
        let mut portal = DatArchive::open(&manifest.portal).map_err(|e| e.to_string())?;
        let cell = DatArchive::open(&manifest.cell).map_err(|e| e.to_string())?;
        if portal.header().dataset != 1 || cell.header().dataset != 2 {
            return Err("incorrect region DAT datasets".into());
        }
        let land = RegionLand::decode_prefix(&portal.read(0x13000000).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(Self { portal, cell, land })
    }
    pub fn prepare(
        &mut self,
        request: &RegionActivationRequest,
    ) -> Result<PreparedRegionActivation, String> {
        if request.token == 0 || request.activation_epoch == 0 {
            return Err("invalid activation fence".into());
        }
        let mut content = crate::world_content::prepare(&request.generation, request.landblock)?;
        let catalog = crate::generator_catalog::prepare_generator_catalog(
            &request.generation,
            &content.templates,
            request
                .treasure_assets
                .clone()
                .unwrap_or_else(|| Arc::new(bace_loot::TreasureAssets::default())),
            request.aetheria_drop_rate,
        )?;
        content.templates = catalog.templates.clone();
        let item_spells = crate::generator_spell_assets::prepare_generator_spell_assets(
            &request.generation,
            &mut self.portal,
            &content.templates,
        );
        let (geometry, visibility) = self.prepare_geometry_with_visibility(request.landblock)?;
        let mut physical = BTreeMap::new();
        for (&id, template) in &content.templates {
            physical.insert(id, self.prepare_template(template));
        }
        let mut creatures = BTreeMap::new();
        for (&id, template) in &content.templates {
            if crate::generator_preparation::is_creature_template(template.weenie_type) {
                let prepared = physical.get(&id).expect("complete immutable closure");
                let creature = match (prepared, request.creature_policy) {
                    (Ok(prepared), Some(policy)) => self
                        .prepare_creature(template, prepared, policy)
                        .and_then(|mut creature| {
                            creature.ace_loot = Some(prepare_ace_loot(
                                request,
                                template,
                                &content.templates,
                                &catalog,
                            )?);
                            Ok(creature)
                        }),
                    (Err(error), _) => Err(error.clone()),
                    (_, None) => Err("creature policy/corpse template not prepared".into()),
                };
                creatures.insert(id, creature);
            }
        }
        Ok(PreparedRegionActivation {
            source_manifest: request.content_hash,
            npc_recovery: BTreeMap::new(),
            generation: request.generation.clone(),
            visibility,
            catalog,
            item_spells,
            creatures,
            fence: RegionActivationFence {
                token: request.token,
                activation_epoch: request.activation_epoch,
                landblock: request.landblock,
                content_generation: request.generation.revision(),
            },
            content,
            geometry,
            physical,
        })
    }
    pub fn prepare_geometry(&mut self, landblock: u16) -> Result<Arc<GeometryRegion>, String> {
        self.prepare_geometry_with_visibility(landblock)
            .map(|(geometry, _)| geometry)
    }
    pub fn prepare_geometry_with_visibility(
        &mut self,
        landblock: u16,
    ) -> Result<(Arc<GeometryRegion>, Vec<bace_world::PreparedCellVisibility>), String> {
        let base = u32::from(landblock) << 16;
        let mut budget = 64 * 1024 * 1024usize;
        let block = Landblock::decode(&read(&mut self.cell, base | 0xffff, &mut budget)?)
            .map_err(|e| e.to_string())?;
        let info = if self.cell.records().contains_key(&(base | 0xfffe)) {
            Some(
                LandblockInfo::decode(&read(&mut self.cell, base | 0xfffe, &mut budget)?)
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let ids: Vec<_> = self
            .cell
            .records()
            .range((base | 0x100)..=(base | 0xfffd))
            .map(|(&id, _)| id)
            .collect();
        if ids.len() > 960 {
            return Err("region cell capacity".into());
        }
        let mut indoors = Vec::new();
        let mut environments = BTreeMap::new();
        let mut pending = Vec::new();
        if let Some(info) = &info {
            pending.extend(info.objects.iter().map(|o| o.id));
            pending.extend(info.buildings.iter().map(|b| b.model));
        }
        for id in ids {
            let cell = EnvCell::decode(&read(&mut self.cell, id, &mut budget)?)
                .map_err(|e| e.to_string())?;
            if let std::collections::btree_map::Entry::Vacant(entry) =
                environments.entry(cell.environment_id)
            {
                entry.insert(
                    Environment::decode(&read(&mut self.portal, cell.environment_id, &mut budget)?)
                        .map_err(|e| e.to_string())?,
                );
            }
            pending.extend(cell.static_objects.iter().map(|o| o.id));
            indoors.push(cell);
        }
        let mut seen = BTreeSet::new();
        let mut graphics = BTreeMap::new();
        let mut setups = BTreeMap::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            if seen.len() > 4096 {
                return Err("static asset closure limit".into());
            }
            let bytes = read(&mut self.portal, id, &mut budget)?;
            match id >> 24 {
                1 => {
                    graphics.insert(
                        id,
                        GraphicsObject::decode(&bytes).map_err(|e| e.to_string())?,
                    );
                }
                2 => {
                    let setup = CollisionSetup::decode(&bytes).map_err(|e| e.to_string())?;
                    pending.extend(setup.parts.iter().copied());
                    setups.insert(id, setup);
                }
                _ => return Err("unsupported static asset kind".into()),
            }
        }
        let mut outdoors = crate::region_geometry::outdoor_block(
            &block,
            info.as_ref(),
            &self.land,
            &graphics,
            &setups,
        )?;
        let mut cells = Vec::new();
        for cell in &indoors {
            cells.push(crate::region_geometry::indoor_cell_with_assets(
                cell,
                &environments[&cell.environment_id],
                &graphics,
                &setups,
                &mut outdoors,
            )?);
        }
        cells.extend(outdoors);
        let buildings = info
            .as_ref()
            .map(crate::region_geometry::building_cells)
            .transpose()?
            .unwrap_or_default();
        let region = GeometryRegion::prepare(cells)
            .and_then(|r| {
                r.with_landblock_metric(self.land.square_length, self.land.landblock_length)
            })
            .and_then(|r| r.with_building_cells(&buildings))
            .map_err(|e| e.to_string())?;
        let visibility = indoors
            .iter()
            .map(|cell| {
                let visible: BTreeSet<_> = cell
                    .visible_cells
                    .iter()
                    .map(|id| bace_types::CellId(base | u32::from(*id)))
                    .collect();
                bace_world::PreparedCellVisibility {
                    cell: bace_types::CellId(cell.id),
                    seen_outside: cell.flags & 1 != 0,
                    visible_cells: visible.into_iter().collect::<Vec<_>>().into(),
                }
            })
            .collect();
        Ok((Arc::new(region), visibility))
    }
    pub fn prepare_creature(
        &mut self,
        template: &bace_content::WeenieTemplate,
        physical: &PreparedPhysicalTemplate,
        policy: crate::generator_preparation::CreatureAdmissionPolicy,
    ) -> Result<bace_simulation::GeneratedNpcTemplate, String> {
        let did = |id| {
            template
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value)
                .ok_or_else(|| {
                    format!(
                        "creature {} ({}) missing DAT property {}",
                        template.weenie_id, template.class_name, id
                    )
                })
        };
        let mut budget = 32 * 1024 * 1024usize;
        let motions = MotionTable::decode(&read(&mut self.portal, did(2)?, &mut budget)?)
            .map_err(|e| e.to_string())?;
        let cmt = template
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 4 && p.value != 0)
            .map(|entry| {
                bace_dat::CombatManeuverTable::decode(&read(
                    &mut self.portal,
                    entry.value,
                    &mut budget,
                )?)
                .map_err(|error| error.to_string())
            })
            .transpose()?;
        let skills =
            bace_dat::SkillTable::decode(&read(&mut self.portal, 0x0e000004, &mut budget)?)
                .map_err(|e| e.to_string())?;
        let vitals =
            bace_dat::VitalTable::decode(&read(&mut self.portal, 0x0e000003, &mut budget)?)
                .map_err(|e| e.to_string())?;
        let quality_filter = bace_dat::QualityFilter::decode(&read(
            &mut self.portal,
            bace_dat::QualityFilter::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let mut animations = BTreeMap::new();
        for data in motions
            .cycles
            .values()
            .chain(motions.modifiers.values())
            .chain(motions.links.values().flat_map(|v| v.values()))
        {
            for segment in &data.animations {
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    animations.entry(segment.animation_id)
                {
                    entry.insert(
                        Animation::decode(&read(
                            &mut self.portal,
                            segment.animation_id,
                            &mut budget,
                        )?)
                        .map_err(|e| e.to_string())?,
                    );
                }
            }
        }
        crate::generator_preparation::prepare_creature(
            template,
            crate::generator_preparation::CreatureAdmissionAssets {
                motions: &motions,
                maneuvers: cmt.as_ref(),
                animations: &animations,
                skills: &skills,
                quality_filter: &quality_filter,
                vitals: &vitals,
                physical,
            },
            policy,
        )
    }
    pub fn prepare_template(
        &mut self,
        template: &bace_content::WeenieTemplate,
    ) -> Result<PreparedPhysicalTemplate, String> {
        let did = |id| {
            template
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value)
        };
        let mut budget = 16 * 1024 * 1024usize;
        let setup_id = did(1).ok_or("template has no collision setup")?;
        let setup = CollisionSetup::decode(&read(&mut self.portal, setup_id, &mut budget)?)
            .map_err(|e| e.to_string())?;
        let scale = template
            .properties
            .floats
            .iter()
            .find(|p| p.id == 39)
            .map_or(1.0, |p| p.value) as f32;
        let shape = crate::world_admission::prepare_collision_shape(&setup, scale)?;
        let locomotion = (|| {
            let id = did(2).unwrap_or(setup.default_motion_table);
            let table = MotionTable::decode(&read(&mut self.portal, id, &mut budget)?)
                .map_err(|e| e.to_string())?;
            let mut animations = BTreeMap::new();
            for data in table.cycles.values() {
                for segment in &data.animations {
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        animations.entry(segment.animation_id)
                    {
                        entry.insert(
                            Animation::decode(&read(
                                &mut self.portal,
                                segment.animation_id,
                                &mut budget,
                            )?)
                            .map_err(|e| e.to_string())?,
                        );
                    }
                }
            }
            crate::world_admission::prepare_animated_locomotion(
                &table,
                table.default_style,
                &animations,
                scale,
            )
        })();
        Ok(PreparedPhysicalTemplate { shape, locomotion })
    }
}
fn read(archive: &mut DatArchive, id: u32, budget: &mut usize) -> Result<Vec<u8>, String> {
    let size = archive
        .records()
        .get(&id)
        .ok_or_else(|| format!("missing DAT record {id:08x}"))?
        .size as usize;
    *budget = budget.checked_sub(size).ok_or("region asset byte budget")?;
    archive.read(id).map_err(|e| e.to_string())
}

fn prepare_ace_loot(
    request: &RegionActivationRequest,
    source: &Arc<bace_content::WeenieTemplate>,
    templates: &BTreeMap<u32, Arc<bace_content::WeenieTemplate>>,
    catalog: &crate::generator_catalog::PreparedGeneratorCatalog,
) -> Result<bace_simulation::AceCreatureLootPolicy, String> {
    use sha2::Digest;
    if request.content_hash == [0; 32] {
        return Err("missing accepted content generation hash".into());
    }
    let death_id = source
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == 35)
        .map(|p| p.value)
        .filter(|v| *v != 0);
    let mut hash = sha2::Sha256::new();
    hash.update(bace_content_tools::compile_template(source).map_err(|e| e.to_string())?);
    let treasure = if let Some(id) = death_id {
        let profile = catalog
            .death
            .get(&id)
            .ok_or_else(|| format!("missing creature death table {id}"))?;
        hash.update(bace_content_tools::compile_world_record(
            &bace_content::WorldRecordV1::TreasureDeath(profile.clone()),
        )?);
        match catalog.treasure.get(&id) {
            Some(crate::generator_treasure::PreparedGeneratorTreasure::Death(treasure)) => {
                Some(treasure.clone())
            }
            _ => return Err(format!("missing prepared creature death table {id}")),
        }
    } else {
        None
    };
    let mut all = request
        .treasure_assets
        .as_ref()
        .map_or_else(BTreeMap::new, |a| a.templates.clone());
    all.extend(templates.iter().map(|(&id, t)| (id, t.clone())));
    let level = source
        .properties
        .ints
        .iter()
        .find(|p| p.id == 25)
        .map_or(1, |p| p.value);
    Ok(bace_simulation::AceCreatureLootPolicy {
        initial_items: vec![],
        initial_parents: vec![],
        initial_ids: vec![],
        source: source.clone(),
        treasure,
        templates: Arc::new(all),
        profile_id: death_id.unwrap_or(source.weenie_id),
        profile_revision: hash.finalize().into(),
        content_generation: request.content_hash,
        rare: None,
        rare_profile_revision: None,
        creature_level: u32::try_from(level)
            .ok()
            .filter(|v| *v > 0)
            .ok_or("invalid creature level")?,
    })
}

mod npc_motion;
