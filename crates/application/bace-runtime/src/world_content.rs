//! Bounded off-simulation preparation of region content from immutable packs.
//! Prepared content is not a geometry admission token and cannot install a pose.
use bace_content::{EncounterRowV1, LandblockInstanceRowV1, WeenieTemplate, WorldRecordV1};
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    thread,
};

#[derive(Clone)]
pub struct PreparedInstance {
    pub source: LandblockInstanceRowV1,
    pub template: Arc<WeenieTemplate>,
    pub links: Vec<bace_content::InstanceLinkTargetV1>,
}
#[derive(Clone)]
pub struct PreparedEncounter {
    pub source: EncounterRowV1,
    pub template: Arc<WeenieTemplate>,
}
pub struct PreparedRegion {
    /// Bounded immutable generator/create-list template closure, including roots.
    pub templates: BTreeMap<u32, Arc<WeenieTemplate>>,
    pub content_generation: u64,
    pub landblock: u16,
    pub instances: Vec<PreparedInstance>,
    pub encounters: Vec<PreparedEncounter>,
}
#[derive(Clone, Copy, Debug)]
pub struct RegionRequest {
    pub token: u64,
    pub landblock: u16,
}
pub struct RegionCompletion {
    pub request: RegionRequest,
    pub result: Result<PreparedRegion, String>,
}

/// Separate bounded capacity from pack writes, persistence and network DDD.
pub struct WorldContentWorker {
    jobs: mpsc::SyncSender<RegionRequest>,
    results: mpsc::Receiver<RegionCompletion>,
    thread: thread::JoinHandle<()>,
}
impl WorldContentWorker {
    pub fn start(generation: Arc<PackGeneration>, capacity: usize) -> Result<Self, String> {
        if !(1..=4).contains(&capacity) {
            return Err("world preparation capacity must be 1..=4".into());
        }
        let (jobs, inbox) = mpsc::sync_channel::<RegionRequest>(capacity);
        let (outbox, results) = mpsc::sync_channel(capacity);
        let thread = thread::Builder::new()
            .name("bace-world-assets".into())
            .spawn(move || {
                while let Ok(request) = inbox.recv() {
                    let result = prepare(&generation, request.landblock);
                    if outbox.send(RegionCompletion { request, result }).is_err() {
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
    pub fn try_submit(&self, request: RegionRequest) -> Result<(), RegionRequest> {
        self.jobs.try_send(request).map_err(|e| match e {
            mpsc::TrySendError::Full(request) | mpsc::TrySendError::Disconnected(request) => {
                request
            }
        })
    }
    pub fn try_recv(&self) -> Result<RegionCompletion, mpsc::TryRecvError> {
        self.results.try_recv()
    }
    pub fn shutdown(self) -> Result<Vec<RegionCompletion>, String> {
        let Self {
            jobs,
            results,
            thread,
        } = self;
        drop(jobs);
        let completions = results.into_iter().collect();
        thread
            .join()
            .map_err(|_| "world preparation worker panicked".to_string())?;
        Ok(completions)
    }
}

pub(crate) fn prepare(
    generation: &PackGeneration,
    landblock: u16,
) -> Result<PreparedRegion, String> {
    let mut budget = 32 * 1024 * 1024;
    let index = match generation
        .lookup(PackKey {
            namespace: 2,
            id: u64::from(landblock),
        })
        .map_err(|e| e.to_string())?
    {
        PackLookup::Record(record) => bace_content_tools::decode_landblock_index(record.bytes())?,
        PackLookup::Missing | PackLookup::Tombstone => {
            return Ok(PreparedRegion {
                templates: BTreeMap::new(),
                content_generation: generation.revision(),
                landblock,
                instances: Vec::new(),
                encounters: Vec::new(),
            });
        }
    };
    if index.landblock != landblock
        || index.instance_ids.len() + index.encounter_ids.len() > 4096
        || index.instance_ids.windows(2).any(|p| p[0] >= p[1])
        || index.encounter_ids.windows(2).any(|p| p[0] >= p[1])
    {
        return Err("region identity/count limit".into());
    }
    let mut templates = BTreeMap::new();
    let mut region = PreparedRegion {
        templates: BTreeMap::new(),
        content_generation: generation.revision(),
        landblock,
        instances: Vec::with_capacity(index.instance_ids.len()),
        encounters: Vec::with_capacity(index.encounter_ids.len()),
    };
    for id in index.instance_ids {
        let record = row(
            generation,
            PackKey {
                namespace: 20,
                id: u64::from(id),
            },
            &mut budget,
        )?;
        let WorldRecordV1::LandblockInstance(source) = record else {
            return Err("instance index points to wrong kind".into());
        };
        if source.guid != id || source.obj_cell_id >> 16 != u32::from(landblock) {
            return Err("instance regional identity mismatch".into());
        }
        let template = template(
            generation,
            source.weenie_class_id,
            &mut templates,
            &mut budget,
        )?;
        let links = match generation
            .lookup(PackKey {
                namespace: 3,
                id: u64::from(id),
            })
            .map_err(|e| e.to_string())?
        {
            PackLookup::Record(record) => {
                budget = budget
                    .checked_sub(record.bytes().len())
                    .ok_or("region link byte budget")?;
                let links = bace_content_tools::decode_instance_link_index(record.bytes())?;
                if links.parent_guid != id || links.children.len() > 4096 {
                    return Err("instance link identity/count limit".into());
                }
                links.children
            }
            PackLookup::Missing | PackLookup::Tombstone => Vec::new(),
        };
        region.instances.push(PreparedInstance {
            source,
            template,
            links,
        });
    }
    for id in index.encounter_ids {
        let record = row(
            generation,
            PackKey {
                namespace: 17,
                id: u64::from(id),
            },
            &mut budget,
        )?;
        let WorldRecordV1::Encounter(source) = record else {
            return Err("encounter index points to wrong kind".into());
        };
        if source.id != id || source.landblock != i32::from(landblock) {
            return Err("encounter regional identity mismatch".into());
        }
        let template = template(
            generation,
            source.weenie_class_id,
            &mut templates,
            &mut budget,
        )?;
        region
            .encounters
            .push(PreparedEncounter { source, template });
    }
    let mut checked = std::collections::BTreeSet::new();
    while let Some((&id, value)) = templates.iter().find(|(id, _)| !checked.contains(*id)) {
        checked.insert(id);
        let children: Vec<_> = value
            .properties
            .generators
            .iter()
            .filter(|g| g.where_create & 0x40 == 0)
            .map(|g| g.weenie_class_id)
            .chain(
                value
                    .properties
                    .create_list
                    .iter()
                    .map(|g| g.weenie_class_id),
            )
            .collect();
        for child in children.into_iter().filter(|id| *id != 0 && *id != 3666) {
            template(generation, child, &mut templates, &mut budget)?;
        }
        if templates.len() > 4096 {
            return Err("generator template closure limit".into());
        }
    }
    region.templates = templates;
    Ok(region)
}

fn row(
    generation: &PackGeneration,
    key: PackKey,
    budget: &mut usize,
) -> Result<WorldRecordV1, String> {
    let PackLookup::Record(record) = generation.lookup(key).map_err(|e| e.to_string())? else {
        return Err("missing regional world record".into());
    };
    *budget = budget
        .checked_sub(record.bytes().len())
        .ok_or("region byte budget")?;
    bace_content_tools::decode_world_record(record.bytes())
}
fn template(
    generation: &PackGeneration,
    id: u32,
    cache: &mut BTreeMap<u32, Arc<WeenieTemplate>>,
    budget: &mut usize,
) -> Result<Arc<WeenieTemplate>, String> {
    if let Some(template) = cache.get(&id) {
        return Ok(template.clone());
    }
    let PackLookup::Record(record) = generation
        .lookup(PackKey {
            namespace: 1,
            id: u64::from(id),
        })
        .map_err(|e| e.to_string())?
    else {
        return Err(format!(
            "missing required template {id}; region admission blocked"
        ));
    };
    *budget = budget
        .checked_sub(record.bytes().len())
        .ok_or("region byte budget")?;
    let value = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
    if value.weenie_id != id {
        return Err("template index identity mismatch".into());
    }
    let value = Arc::new(value);
    cache.insert(id, value.clone());
    Ok(value)
}
