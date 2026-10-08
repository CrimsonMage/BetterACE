//! Retained publication I/O. A durable accepted manifest is adopted before any
//! later cold request; in-flight requests retain their original immutable pack.
use super::*;
use crate::pack_io::{PackIoWorker, PackJob, PreparedPack};
use bace_storage_codec::{PackKey, PackLimits, PackManifest};
#[cfg(test)]
mod tests;
struct Completion {
    worker: Option<PackIoWorker>,
    pack: PreparedPack,
    changed_keys: Vec<PackKey>,
    result: Result<crate::PublicationResult, String>,
}
pub(super) struct PublicationRuntime {
    job: Option<Job<Completion>>,
    adopt: Option<(PreparedPack, Vec<PackKey>)>,
    recover: bool,
    retry_at: Duration,
    failure: Option<String>,
}
impl PublicationRuntime {
    pub(super) fn new() -> Self {
        Self {
            job: None,
            adopt: None,
            recover: false,
            retry_at: Duration::ZERO,
            failure: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.job.is_some() || self.adopt.is_some() || self.recover
    }
}
impl GameRuntime {
    pub(super) fn poll_publication(&mut self, elapsed: Duration) -> Result<(), String> {
        if let Some(completion) = ready(&mut self.publication.job) {
            self.bootstrap.pack_io = completion.worker;
            match completion.result {
                Ok(_) => {
                    self.publication.recover = false;
                    self.publication.failure = None;
                    let old_hash = self
                        .bootstrap
                        .pack
                        .manifest
                        .content_hash(PackLimits::default())
                        .map_err(|e| e.to_string())?;
                    let new_hash = completion
                        .pack
                        .manifest
                        .content_hash(PackLimits::default())
                        .map_err(|e| e.to_string())?;
                    if new_hash != old_hash {
                        self.publication.adopt = Some((completion.pack, completion.changed_keys));
                    }
                }
                Err(error) => {
                    // The adapter future retains the only writer. A failed or
                    // uncertain commit must reload durable authority before retry.
                    self.publication.recover = true;
                    self.publication.failure = Some(error.clone());
                    self.publication.retry_at = elapsed + Duration::from_secs(1);
                    return Err(format!("content publication held: {error}"));
                }
            }
        }
        if let Some((pack, changed_keys)) = self.publication.adopt.as_ref() {
            let Some(world) = self.world.as_mut() else {
                return Ok(());
            };
            let hash = pack
                .manifest
                .content_hash(PackLimits::default())
                .map_err(|e| e.to_string())?;
            world
                .regions
                .adopt_content_generation(pack.generation.clone(), hash, changed_keys)?;
            self.bootstrap.pack = self
                .publication
                .adopt
                .take()
                .expect("checked prepared pack")
                .0;
        }
        if self.publication.job.is_some()
            || elapsed < self.publication.retry_at
            || (self.draining && !self.publication.recover)
        {
            return Ok(());
        }
        let Some(worker) = self.bootstrap.pack_io.take() else {
            return Err("content writer unavailable; publication owner retained".into());
        };
        let active = PreparedPack {
            manifest: self.bootstrap.pack.manifest.clone(),
            generation: self.bootstrap.pack.generation.clone(),
        };
        let store = self.bootstrap.store.clone();
        let recover = self.publication.recover;
        let directory = self.bootstrap.config.pack_directory.clone();
        let pressure = self.bootstrap.save_pressure.clone();
        self.publication.retry_at = elapsed + Duration::from_secs(1);
        self.publication.job = Some(Box::pin(async move {
            if recover {
                return recover_authority(store, worker, active, directory, pressure).await;
            }
            let mut worker = worker;
            let mut pack = active;
            let (delivery, mut receiver) = tokio::sync::mpsc::channel(1);
            let result = crate::native_publication::publish_native_once(
                &store,
                &mut worker,
                &mut pack,
                &delivery,
            )
            .await;
            let (result, changed_keys) = match result {
                Ok(
                    value @ (crate::PublicationResult::Idle
                    | crate::PublicationResult::Rejected { .. }),
                ) => (Ok(value), Vec::new()),
                Ok(value) => match changed_keys(&pack).await {
                    Ok(keys) => (Ok(value), keys),
                    Err(error) => (Err(error), Vec::new()),
                },
                Err(error) => (Err(error), Vec::new()),
            };
            // Delivery is owned by this same retained operation. Pack adoption
            // happens above only after its durable receipt has returned.
            let _delivered = receiver.try_recv().ok();
            Completion {
                worker: Some(worker),
                pack,
                changed_keys,
                result,
            }
        }));
        Ok(())
    }
}
async fn recover_authority(
    store: bace_db_postgres::PgStore,
    worker: PackIoWorker,
    mut pack: PreparedPack,
    directory: Option<std::path::PathBuf>,
    pressure: Arc<std::sync::atomic::AtomicBool>,
) -> Completion {
    // Drain/join away from the adapter and simulation. Unaccepted immutable
    // outputs are not authority; candidate bytes remain in the durable journal.
    let stopped = tokio::task::spawn_blocking(move || worker.shutdown()).await;
    if let Err(error) = stopped.map_err(|e| e.to_string()).and_then(|v| v) {
        return Completion {
            worker: None,
            pack,
            changed_keys: Vec::new(),
            result: Err(error),
        };
    }
    let mut worker = None;
    let result = async {
        worker = Some(PackIoWorker::start(
            directory.ok_or("pack directory missing")?,
            2,
            pressure,
        )?);
        let durable = store
            .active_generation()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("accepted content generation missing")?;
        let manifest = PackManifest::decode(&durable.manifest_bytes, PackLimits::default())
            .map_err(|e| e.to_string())?;
        if manifest
            .content_hash(PackLimits::default())
            .map_err(|e| e.to_string())?
            != durable.manifest_hash
            || manifest.base.generation != durable.base_hash
        {
            return Err("accepted content metadata mismatch".into());
        }
        let io = worker.as_ref().expect("started writer");
        let id = io
            .try_submit_tracked(PackJob::Open { manifest })
            .map_err(|_| "accepted content open rejected")?;
        loop {
            match io.try_recv() {
                Ok(completion) if completion.job_id == id => {
                    pack = completion.result?;
                    break;
                }
                Ok(_) => return Err("unexpected content recovery completion".into()),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("content recovery writer disconnected".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(2)).await
                }
            }
        }
        Ok(crate::PublicationResult::Idle)
    }
    .await;
    let (result, changed_keys) = match result {
        Ok(value) => match changed_keys(&pack).await {
            Ok(keys) => (Ok(value), keys),
            Err(error) => (Err(error), Vec::new()),
        },
        Err(error) => (Err(error), Vec::new()),
    };
    Completion {
        worker,
        pack,
        changed_keys,
        result,
    }
}
async fn changed_keys(pack: &PreparedPack) -> Result<Vec<PackKey>, String> {
    let generation = pack.generation.clone();
    tokio::task::spawn_blocking(move || generation.newest_delta_keys().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}
