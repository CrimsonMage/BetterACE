//! Bounded mixed content publication through the durable journal and pack owner.
use crate::PublicationResult;
use crate::pack_io::{PackIoWorker, PackJob, PreparedPack};
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{ContentCandidate, MappedGeneration, NativeContentCandidate};
use bace_storage_codec::{
    PackGeneration, PackKey, PackLimits, PackLookup, PackManifest, PackRecord,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Semaphore, mpsc};
// One bounded profile preparation lane per process. The blocking task retains
// this permit even if its awaiting async caller is cancelled.
static PREPARATION: Semaphore = Semaphore::const_new(1);

/// The caller serializes access to this pack worker. On any uncertain database
/// result it must reload the accepted manifest before resuming. A queued output
/// is adopted by the simulation owner at its next content boundary.
///
/// Weenies, world rows, native profiles, ClothingBase overrides and tombstones
/// from one SQL transaction are validated and mapped together, including derived
/// indexes. No partial batch is accepted.
pub async fn publish_native_once(
    store: &PgStore,
    worker: &mut PackIoWorker,
    active: &mut PreparedPack,
    delivery: &mpsc::Sender<Arc<PackGeneration>>,
) -> Result<PublicationResult, String> {
    let old = store
        .active_generation()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("native publication requires an accepted world pack")?;
    if active
        .manifest
        .content_hash(PackLimits::default())
        .map_err(|e| e.to_string())?
        != old.manifest_hash
    {
        return Err("stale native publication base; reload accepted manifest".into());
    }
    let permit = PREPARATION
        .acquire()
        .await
        .map_err(|_| "profile preparation lane closed")?;
    let pending = match store.pending_native_publication().await {
        Ok(pending) => pending,
        Err(StoreError::PublicationTooLarge(revision)) => {
            let reason = "publication exceeds 4096 records or 16 MiB".to_string();
            store
                .reject(revision, &reason)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(PublicationResult::Rejected { revision, reason });
        }
        Err(error) => return Err(error.to_string()),
    };
    let Some(pending) = pending else {
        return Ok(PublicationResult::Idle);
    };
    if pending
        .expected_manifest_hash
        .is_some_and(|expected| expected != old.manifest_hash)
    {
        let reason = "reviewed inbox base changed before publication".to_string();
        store
            .reject(pending.revision, &reason)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(PublicationResult::Rejected {
            revision: pending.revision,
            reason,
        });
    }
    let mut weenies = Vec::with_capacity(pending.weenie_candidates);
    let mut after = 0;
    while weenies.len() < pending.weenie_candidates {
        let page = store
            .candidate_page(pending.revision, after, 16)
            .await
            .map_err(|e| e.to_string())?;
        if page.is_empty() || weenies.len() + page.len() > pending.weenie_candidates {
            return Err("incomplete immutable weenie publication".into());
        }
        after = page.last().expect("nonempty page").wcid;
        weenies.extend(page);
    }
    if weenies.is_empty() && pending.candidates.is_empty() && pending.mapped_candidates.is_empty() {
        return Err("empty content publication".into());
    }
    let generation = active.generation.clone();
    let (permit, checked) = tokio::task::spawn_blocking(move || {
        let checked = validate_candidates(
            &generation,
            weenies,
            pending.candidates,
            pending.mapped_candidates,
        );
        (permit, checked)
    })
    .await
    .map_err(|e| e.to_string())?;
    drop(permit);
    let records = match checked {
        Ok(records) => records,
        Err(reason) => {
            let reason: String = reason.chars().take(1024).collect();
            store
                .reject(pending.revision, &reason)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(PublicationResult::Rejected {
                revision: pending.revision,
                reason,
            });
        }
    };
    let permit = delivery
        .reserve()
        .await
        .map_err(|_| "content receiver closed")?;
    let mut parent = old.manifest_hash;
    if active.manifest.deltas.len() == 2 {
        let compacted = run_pack_job(
            worker,
            PackJob::Compact {
                current: active.manifest.clone(),
            },
        )
        .await?;
        let layout = durable_generation(&compacted.manifest, parent, old.accepted_revision)?;
        store
            .accept_mapped(None, &layout)
            .await
            .map_err(|e| e.to_string())?;
        parent = layout.manifest_hash;
        *active = compacted;
    }
    let prepared = run_pack_job(
        worker,
        PackJob::Delta {
            current: active.manifest.clone(),
            records,
        },
    )
    .await?;
    let durable = durable_generation(&prepared.manifest, parent, pending.revision)?;
    store
        .accept_mapped(Some(pending.revision), &durable)
        .await
        .map_err(|e| e.to_string())?;
    permit.send(prepared.generation.clone());
    *active = prepared;
    Ok(PublicationResult::Accepted {
        revision: pending.revision,
    })
}

fn durable_generation(
    manifest: &PackManifest,
    parent: [u8; 32],
    revision: i64,
) -> Result<MappedGeneration, String> {
    let limits = PackLimits::default();
    Ok(MappedGeneration {
        manifest_hash: manifest.content_hash(limits).map_err(|e| e.to_string())?,
        parent_hash: Some(parent),
        base_hash: manifest.base.generation,
        accepted_revision: revision,
        manifest_bytes: manifest.encode(limits).map_err(|e| e.to_string())?,
    })
}
async fn run_pack_job(worker: &mut PackIoWorker, job: PackJob) -> Result<PreparedPack, String> {
    let job_id = worker
        .try_submit_tracked(job)
        .map_err(|_| "pack worker admission full or closed")?;
    loop {
        match worker.try_recv() {
            Ok(completion) if completion.job_id == job_id => return completion.result,
            Ok(_) => return Err("stale pack completion after cancellation; discard worker and reload accepted manifest".into()),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("pack worker disconnected; publication remains pending".into());
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(2)).await
            }
        }
    }
}

pub(crate) fn validate_candidates(
    generation: &PackGeneration,
    weenies: Vec<ContentCandidate>,
    candidates: Vec<NativeContentCandidate>,
    mapped_candidates: Vec<bace_persistence::MappedContentCandidate>,
) -> Result<Vec<PackRecord>, String> {
    let mut records =
        Vec::with_capacity(weenies.len() + candidates.len() + mapped_candidates.len() + 4);
    let mut staged = std::collections::BTreeMap::new();
    let (mapped_records, removed, world_references) =
        crate::mapped_validation::validate_mapped(generation, mapped_candidates)?;
    records.extend(mapped_records);
    for candidate in weenies {
        let template = bace_content_tools::decode(&candidate.bytes).map_err(|e| e.to_string())?;
        if template.weenie_id != candidate.wcid
            || template.class_name != candidate.class_name
            || i32::try_from(template.weenie_type).ok() != Some(candidate.weenie_type)
            || staged.contains_key(&candidate.wcid)
            || removed.contains(&candidate.wcid)
        {
            return Err("weenie scalar identity mismatch".into());
        }
        records.push(PackRecord {
            key: PackKey {
                namespace: 1,
                id: u64::from(candidate.wcid),
            },
            schema: 1,
            value: Some(candidate.bytes),
        });
        staged.insert(candidate.wcid, template);
    }
    for id in world_references {
        if id == 0 {
            continue;
        }
        if !staged.contains_key(&id) {
            validate_template(generation, id, &removed)?;
        }
    }
    if !staged.is_empty() || !removed.is_empty() {
        let PackLookup::Record(record) = generation
            .lookup(PackKey {
                namespace: 49,
                id: 1,
            })
            .map_err(|e| e.to_string())?
        else {
            return Err("missing template class-name index".into());
        };
        let mut classes = bace_content_tools::decode_template_classes(record.bytes())?;
        let old_classes = classes.clone();
        classes.entries.retain(|entry| {
            !staged.contains_key(&entry.template) && !removed.contains(&entry.template)
        });
        classes.entries.extend(staged.iter().map(|(&id, template)| {
            bace_content::TemplateClassIdentityV1 {
                template: id,
                class_name: template.class_name.clone(),
            }
        }));
        classes.entries.sort_unstable_by_key(|entry| entry.template);
        classes.validate()?;
        if classes != old_classes {
            records.push(PackRecord {
                key: PackKey {
                    namespace: 49,
                    id: 1,
                },
                schema: 1,
                value: Some(bace_content_tools::compile_template_classes(&classes)?),
            });
        }
        let mut index = crate::creation_profile::load_creature_names(generation)?;
        let before = index.clone();
        index
            .entries
            .retain(|entry| !removed.contains(&entry.template));
        for (&id, template) in &staged {
            let name = (template.weenie_type == 10)
                .then(|| template.properties.strings.iter().find(|p| p.id == 1))
                .flatten();
            match (
                index
                    .entries
                    .binary_search_by_key(&id, |entry| entry.template),
                name,
            ) {
                (Ok(position), Some(name)) => index.entries[position].name = name.value.clone(),
                (Ok(position), None) => {
                    index.entries.remove(position);
                }
                (Err(position), Some(name)) => index.entries.insert(
                    position,
                    bace_content::CreatureNameV1 {
                        template: id,
                        name: name.value.clone(),
                    },
                ),
                (Err(_), None) => {}
            }
        }
        if index != before {
            records.push(PackRecord {
                key: PackKey {
                    namespace: 48,
                    id: 1,
                },
                schema: 1,
                value: Some(bace_content_tools::compile_creature_names(&index)?),
            });
        }
    }
    for c in candidates {
        if c.schema != 1 {
            return Err("unsupported native schema".into());
        }
        match c.namespace {
            46 => {
                let graph = bace_content_tools::decode_loot_graph(&c.bytes)?;
                if graph.id != c.id {
                    return Err("loot scalar identity mismatch".into());
                }
                for node in graph.nodes {
                    if let Some(item) = node.item
                        && !staged.contains_key(&item.template)
                    {
                        validate_template(generation, item.template, &removed)?;
                    }
                }
            }
            47 => {
                let rare = bace_content_tools::decode_rare_profile(&c.bytes)?;
                if rare.id != c.id {
                    return Err("rare scalar identity mismatch".into());
                }
                for tier in rare.tiers {
                    for item in tier.items {
                        if !staged.contains_key(&item.template) {
                            validate_template(generation, item.template, &removed)?;
                        }
                    }
                }
            }
            _ => return Err("unsupported native namespace".into()),
        }
        records.push(PackRecord {
            key: PackKey {
                namespace: c.namespace,
                id: u64::from(c.id),
            },
            schema: c.schema,
            value: Some(c.bytes),
        });
    }
    // The journal bounds input to 16 MiB / 4096 records. Derived identity
    // indexes add at most two 8 MiB payloads plus their checked envelopes.
    // Reject before reserving delivery or submitting immutable pack work.
    let bytes = records.iter().try_fold(0usize, |total, record| {
        total.checked_add(record.value.as_ref().map_or(0, Vec::len))
    });
    if records.len() > 4098 || bytes.is_none_or(|n| n > 40 * 1024 * 1024) {
        return Err("compiled publication exceeds bounded pack capacity".into());
    }
    records.sort_unstable_by_key(|record| record.key);
    if records.windows(2).any(|pair| pair[0].key == pair[1].key) {
        return Err("duplicate compiled publication key".into());
    }
    Ok(records)
}
fn validate_template(
    generation: &PackGeneration,
    id: u32,
    removed: &std::collections::BTreeSet<u32>,
) -> Result<(), String> {
    if removed.contains(&id) {
        return Err(format!("content references removed template {id}"));
    }
    let PackLookup::Record(record) = generation
        .lookup(PackKey {
            namespace: 1,
            id: u64::from(id),
        })
        .map_err(|e| e.to_string())?
    else {
        return Err(format!("loot references missing template {id}"));
    };
    if record.schema() != 1 {
        return Err("unsupported referenced template schema".into());
    }
    let template = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
    if template.weenie_id != id {
        return Err("referenced template identity mismatch".into());
    }
    Ok(())
}
