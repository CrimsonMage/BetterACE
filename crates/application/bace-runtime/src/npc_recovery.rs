//! Bounded recovery of canonical source heads before regional/static admission.
//! `program_hash` is SHA-256 of the exact compiled template record (namespace1,
//! original WCID) in the accepted manifest. Registration uses the same helper.
use crate::npc_persistence::{NpcCheckpointBinding, restore_checkpoint};
use bace_db_postgres::{PgStore, StoredNpcSourceHead};
use bace_emotes::{NativeLimits, NativeProgram};
use bace_entity::{EntityProperties, PropertyFamily as F, PropertyValue as V};
use bace_simulation::NpcSourceCheckpoint;
use bace_storage_codec::{
    NpcWorkflowSaveV3, PackGeneration, PackKey, PackLimits, PackLookup, PackManifest,
};
use bace_types::EntityId;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

#[derive(Clone)]
pub struct PreparedNpcSource {
    pub authored: Arc<bace_content::WeenieV1>,
    pub template: u32,
    pub retained_bytes: usize,
    pub program_hash: [u8; 32],
    pub program: Arc<NativeProgram>,
    pub properties: EntityProperties,
}
/// Cold bounded lookup; never called on the simulation thread. No whole-world
/// scan or GUID-to-template inference is used, including for deleted sources.
pub fn prepare_npc_source(
    generation: &PackGeneration,
    template: u32,
) -> Result<PreparedNpcSource, String> {
    if template == 0 {
        return Err("NPC source WCID is missing".into());
    }
    let PackLookup::Record(record) = generation
        .lookup(PackKey {
            namespace: 1,
            id: u64::from(template),
        })
        .map_err(|e| e.to_string())?
    else {
        return Err("pinned NPC template is missing".into());
    };
    if record.bytes().len() > 16 * 1024 * 1024 {
        return Err("NPC template byte budget".into());
    }
    let value = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
    if value.weenie_id != template {
        return Err("NPC template index mismatch".into());
    }
    let properties = source_properties(&value)?;
    let program = Arc::new(
        NativeProgram::prepare(value.properties.emotes.clone(), NativeLimits::default())
            .map_err(|e| format!("NPC program: {e:?}"))?,
    );
    Ok(PreparedNpcSource {
        template,
        retained_bytes: record
            .bytes()
            .len()
            .checked_mul(3)
            .ok_or("NPC retained definition size")?,
        authored: Arc::new(value),
        program_hash: Sha256::digest(record.bytes()).into(),
        program,
        properties,
    })
}

/// Inventory only IDs before admitting regions. Heads/checkpoint bytes are read
/// one bounded page at a time later. Admission must suppress these IDs until
/// `PreparedNpcRecovery` identifies live versus archived ownership.
pub async fn pending_source_inventory(
    store: &PgStore,
    capacity: usize,
) -> Result<BTreeSet<EntityId>, String> {
    source_inventory(store, capacity, 0).await
}
/// Includes completed archived heads: finishing a script never resurrects its
/// source. Only explicit later canonical live-source authority removes this tombstone.
pub async fn suppressed_source_inventory(
    store: &PgStore,
    capacity: usize,
) -> Result<BTreeSet<EntityId>, String> {
    source_inventory(store, capacity, 1).await
}
pub async fn archived_source_inventory(
    store: &PgStore,
    capacity: usize,
) -> Result<BTreeSet<EntityId>, String> {
    source_inventory(store, capacity, 2).await
}
async fn source_inventory(
    store: &PgStore,
    capacity: usize,
    mode: u8,
) -> Result<BTreeSet<EntityId>, String> {
    if !(1..=65536).contains(&capacity) {
        return Err("NPC recovery inventory capacity".into());
    }
    let mut ids = BTreeSet::new();
    let mut after = None;
    loop {
        let page = if mode == 2 {
            store.archived_npc_source_ids(after, 256).await
        } else if mode == 1 {
            store.suppressed_npc_source_ids(after, 256).await
        } else {
            store.pending_npc_source_ids(after, 256).await
        }
        .map_err(|e| e.to_string())?;
        if page.is_empty() {
            return Ok(ids);
        }
        for source in page {
            if after.is_some_and(|old| source <= old) || ids.len() == capacity {
                return Err("NPC recovery inventory order/capacity".into());
            }
            ids.insert(EntityId(source));
            after = Some(source);
        }
    }
}

pub struct NpcRecoveryRequest {
    pub head: StoredNpcSourceHead,
    pub manifest: PackManifest,
    pub manifest_hash: [u8; 32],
}
/// SQL ownership stays in PgStore; no database or file work runs in a tick.
pub async fn load_recovery_request(
    store: &PgStore,
    source: EntityId,
) -> Result<NpcRecoveryRequest, String> {
    let head = store
        .npc_source_head(source.0)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("NPC source head disappeared")?;
    let checkpoint =
        NpcWorkflowSaveV3::decode_or_migrate(&head.checkpoint).map_err(|e| e.to_string())?;
    if checkpoint.completed || checkpoint.source_template == 0 {
        return Err(
            "NPC recovery requires active canonical head with explicit original WCID".into(),
        );
    }
    let accepted = store
        .generation_by_hash(checkpoint.content_generation)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("NPC pinned generation is unavailable")?;
    let manifest = PackManifest::decode(&accepted.manifest_bytes, PackLimits::default())
        .map_err(|e| e.to_string())?;
    if manifest
        .content_hash(PackLimits::default())
        .map_err(|e| e.to_string())?
        != checkpoint.content_generation
        || manifest.base.generation != accepted.base_hash
    {
        return Err("NPC historical generation identity mismatch".into());
    }
    Ok(NpcRecoveryRequest {
        head,
        manifest,
        manifest_hash: checkpoint.content_generation,
    })
}
pub struct PreparedNpcRecovery {
    pub binding: NpcCheckpointBinding,
    pub workflow_version: i64,
    pub checkpoint_bytes: usize,
    pub source: PreparedNpcSource,
    pub checkpoint: NpcSourceCheckpoint,
    /// Retain mappings and exact manifest pin while any continuation remains.
    pub generation: Arc<PackGeneration>,
    pub manifest: PackManifest,
    pub participants: BTreeSet<EntityId>,
}
impl PreparedNpcRecovery {
    pub fn archived(&self) -> bool {
        self.checkpoint.archive.is_some()
    }
}
pub struct NpcRecoveryCompletion {
    pub source: EntityId,
    pub result: Result<PreparedNpcRecovery, String>,
}
pub struct NpcRecoveryWorker {
    pending: std::cell::Cell<usize>,
    jobs: mpsc::SyncSender<NpcRecoveryRequest>,
    results: mpsc::Receiver<NpcRecoveryCompletion>,
    thread: thread::JoinHandle<()>,
}
impl NpcRecoveryWorker {
    pub fn start(directory: PathBuf, capacity: usize) -> Result<Self, String> {
        if !(1..=4).contains(&capacity) {
            return Err("NPC recovery worker capacity".into());
        }
        let (jobs, inbox) = mpsc::sync_channel::<NpcRecoveryRequest>(capacity);
        let (outbox, results) = mpsc::sync_channel(capacity);
        let thread = thread::Builder::new()
            .name("bace-npc-recovery".into())
            .spawn(move || {
                while let Ok(request) = inbox.recv() {
                    let source = EntityId(request.head.source);
                    let result = prepare_recovery(&directory, request);
                    if outbox
                        .send(NpcRecoveryCompletion { source, result })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            pending: std::cell::Cell::new(0),
            jobs,
            results,
            thread,
        })
    }
    pub fn try_submit(&self, request: NpcRecoveryRequest) -> Result<(), Box<NpcRecoveryRequest>> {
        self.jobs
            .try_send(request)
            .map_err(|e| match e {
                mpsc::TrySendError::Full(request) | mpsc::TrySendError::Disconnected(request) => {
                    Box::new(request)
                }
            })
            .map(|()| self.pending.set(self.pending.get() + 1))
    }
    pub fn try_recv(&self) -> Result<NpcRecoveryCompletion, mpsc::TryRecvError> {
        self.results
            .try_recv()
            .inspect(|_| self.pending.set(self.pending.get().saturating_sub(1)))
    }
    pub fn has_pending(&self) -> bool {
        self.pending.get() != 0
    }
    pub fn try_shutdown(self) -> Result<thread::JoinHandle<()>, Box<Self>> {
        if self.has_pending() {
            return Err(Box::new(self));
        }
        drop(self.jobs);
        Ok(self.thread)
    }
    pub fn shutdown(self) -> Result<Vec<NpcRecoveryCompletion>, String> {
        let Self {
            jobs,
            results,
            thread,
            ..
        } = self;
        drop(jobs);
        let results = results.into_iter().collect();
        thread.join().map_err(|_| "NPC recovery worker panicked")?;
        Ok(results)
    }
}
fn prepare_recovery(
    directory: &std::path::Path,
    request: NpcRecoveryRequest,
) -> Result<PreparedNpcRecovery, String> {
    let head = request.head;
    let checkpoint =
        NpcWorkflowSaveV3::decode_or_migrate(&head.checkpoint).map_err(|e| e.to_string())?;
    if checkpoint.completed
        || checkpoint.source != head.source
        || checkpoint.source_version != head.source_version
        || checkpoint.source_template != head.source_template
        || checkpoint.invocation != head.invocation
        || checkpoint.content_generation != request.manifest_hash
        || request
            .manifest
            .content_hash(PackLimits::default())
            .map_err(|e| e.to_string())?
            != request.manifest_hash
    {
        return Err("NPC recovery head/manifest mismatch".into());
    }
    let generation = Arc::new(
        request
            .manifest
            .open(directory, PackLimits::default())
            .map_err(|e| e.to_string())?,
    );
    let source = prepare_npc_source(&generation, checkpoint.source_template)?;
    if source.program_hash != checkpoint.program_hash {
        return Err("NPC pinned program hash mismatch".into());
    }
    let binding = NpcCheckpointBinding {
        source_version: head.source_version,
        source_template: source.template,
        invocation: head.invocation,
        source: head.source,
        program_hash: source.program_hash,
        content_generation: request.manifest_hash,
    };
    let checkpoint = restore_checkpoint(binding, checkpoint).map_err(|e| e.to_string())?;
    let mut participants = BTreeSet::new();
    for row in checkpoint
        .vm
        .work
        .iter()
        .chain(checkpoint.vm.pending.iter().map(|p| &p.row))
    {
        if let Some(target) = row.context.target {
            participants.insert(target);
        }
    }
    for effect in &checkpoint.pending {
        if let Some(target) = effect.proposal.context.target {
            participants.insert(target);
        }
    }
    participants.remove(&EntityId(head.source));
    Ok(PreparedNpcRecovery {
        binding,
        workflow_version: head.workflow_version,
        checkpoint_bytes: head.checkpoint.len(),
        source,
        checkpoint,
        generation,
        manifest: request.manifest,
        participants,
    })
}

/// Scalar qualities are read from the live owner, while non-scalar immutable
/// template definitions retain their accepted manifest provenance.
pub fn overlay_npc_qualities(
    source: &bace_content::WeenieV1,
    qualities: &EntityProperties,
) -> Result<bace_content::WeenieV1, String> {
    if qualities
        .retained_bytes()
        .is_none_or(|n| n > 2 * 1024 * 1024)
    {
        return Err("NPC quality byte budget".into());
    }
    let mut value = source.clone();
    value.properties.bools.clear();
    value.properties.ints.clear();
    value.properties.int64s.clear();
    value.properties.floats.clear();
    value.properties.strings.clear();
    for (family, id, field) in qualities.snapshot().1 {
        match (family, field) {
            (F::Bool, V::Bool(field)) => value
                .properties
                .bools
                .push(bace_content::Property { id, value: field }),
            (F::Int, V::Int(field)) => value
                .properties
                .ints
                .push(bace_content::Property { id, value: field }),
            (F::Int64, V::Int64(field)) => value
                .properties
                .int64s
                .push(bace_content::Property { id, value: field }),
            (F::Float, V::Float(field)) => value
                .properties
                .floats
                .push(bace_content::Property { id, value: field }),
            (F::String, V::String(field)) => value
                .properties
                .strings
                .push(bace_content::Property { id, value: field }),
            _ => {}
        }
    }
    value
        .validate(bace_content::ContentLimits::default())
        .map_err(|e| e.to_string())?;
    Ok(value)
}

pub fn source_properties(value: &bace_content::WeenieV1) -> Result<EntityProperties, String> {
    let mut properties = Vec::new();
    properties.extend(
        value
            .properties
            .bools
            .iter()
            .map(|p| (F::Bool, p.id, V::Bool(p.value))),
    );
    properties.extend(
        value
            .properties
            .ints
            .iter()
            .map(|p| (F::Int, p.id, V::Int(p.value))),
    );
    properties.extend(
        value
            .properties
            .int64s
            .iter()
            .map(|p| (F::Int64, p.id, V::Int64(p.value))),
    );
    properties.extend(
        value
            .properties
            .floats
            .iter()
            .map(|p| (F::Float, p.id, V::Float(p.value))),
    );
    properties.extend(
        value
            .properties
            .strings
            .iter()
            .map(|p| (F::String, p.id, V::String(p.value.clone()))),
    );
    EntityProperties::restore_snapshot(0, properties).map_err(|e| format!("NPC properties: {e:?}"))
}
