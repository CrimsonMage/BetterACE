//! Dedicated bounded owner of runtime pack opens, writes and compaction.
//! Publication CAS remains in the database adapter; this worker never makes a
//! candidate active. Asset preparation has separate capacity.
use bace_storage_codec::{PackGeneration, PackLimits, PackManifest, PackRecord};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

pub enum PackJob {
    Open {
        manifest: PackManifest,
    },
    Delta {
        current: PackManifest,
        records: Vec<PackRecord>,
    },
    Compact {
        current: PackManifest,
    },
}

pub struct PreparedPack {
    pub manifest: PackManifest,
    pub generation: Arc<PackGeneration>,
}

/// The original job is returned even on failure, retaining ownership for retry.
pub struct PackCompletion {
    pub job_id: u64,
    pub job: PackJob,
    pub result: Result<PreparedPack, String>,
}

pub struct PackIoWorker {
    jobs: mpsc::SyncSender<(u64, PackJob)>,
    next_job: std::cell::Cell<u64>,
    results: mpsc::Receiver<PackCompletion>,
    thread: thread::JoinHandle<()>,
}

impl PackIoWorker {
    pub fn start(
        directory: PathBuf,
        capacity: usize,
        save_pressure: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        if !(1..=4).contains(&capacity) {
            return Err("pack queue capacity must be 1..=4".into());
        }
        let (jobs, inbox) = mpsc::sync_channel(capacity);
        let (outbox, results) = mpsc::sync_channel(capacity);
        let thread = thread::Builder::new()
            .name("bace-pack-io".into())
            .spawn(move || {
                while let Ok((job_id, job)) = inbox.recv() {
                    let result = execute(&directory, &job, &save_pressure);
                    if outbox
                        .send(PackCompletion {
                            job_id,
                            job,
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
            next_job: std::cell::Cell::new(1),
            results,
            thread,
        })
    }

    pub fn try_submit(&self, job: PackJob) -> Result<(), Box<PackJob>> {
        self.try_submit_tracked(job).map(|_| ())
    }

    /// Correlation survives cancellation of the awaiting adapter. IDs are never
    /// reused on this worker; failed admission returns the original job.
    pub fn try_submit_tracked(&self, job: PackJob) -> Result<u64, Box<PackJob>> {
        let id = self.next_job.get();
        let Some(next) = id.checked_add(1) else {
            return Err(Box::new(job));
        };
        if !valid_job(&job) {
            return Err(Box::new(job));
        }
        self.jobs.try_send((id, job)).map_err(|error| match error {
            mpsc::TrySendError::Full((_, job)) | mpsc::TrySendError::Disconnected((_, job)) => {
                Box::new(job)
            }
        })?;
        self.next_job.set(next);
        Ok(id)
    }

    pub fn try_recv(&self) -> Result<PackCompletion, mpsc::TryRecvError> {
        self.results.try_recv()
    }

    /// Stop admission, drain every admitted completion, and join the I/O owner.
    /// Call outside simulation. Returned candidates are not publication receipts.
    pub fn shutdown(self) -> Result<Vec<PackCompletion>, String> {
        let Self {
            jobs,
            next_job: _,
            results,
            thread,
        } = self;
        drop(jobs);
        let completions = results.into_iter().collect();
        thread
            .join()
            .map_err(|_| "pack worker panicked".to_string())?;
        Ok(completions)
    }
}

fn valid_job(job: &PackJob) -> bool {
    let manifest = match job {
        PackJob::Open { manifest } => manifest,
        PackJob::Delta { current, .. } | PackJob::Compact { current } => current,
    };
    if manifest.encode(PackLimits::default()).is_err() {
        return false;
    }
    if let PackJob::Delta { records, .. } = job {
        if records.is_empty() || records.len() > 4098 {
            return false;
        }
        let total = records.iter().try_fold(0_usize, |sum, record| {
            sum.checked_add(record.value.as_ref().map_or(0, Vec::len))
        });
        if total.is_none_or(|total| total > 40 * 1024 * 1024) {
            return false;
        }
    }
    true
}

fn execute(
    directory: &std::path::Path,
    job: &PackJob,
    pressure: &AtomicBool,
) -> Result<PreparedPack, String> {
    let limits = PackLimits::default();
    let manifest = match job {
        PackJob::Open { manifest } => manifest.clone(),
        PackJob::Delta { current, records } => {
            if current.deltas.len() == 2 {
                return Err("three active packs: compact before another publication".into());
            }
            let _validated = current.open(directory, limits).map_err(|e| e.to_string())?;
            let next = current
                .generation
                .checked_add(1)
                .ok_or("pack generation exhausted")?;
            let descriptor = bace_storage_codec::compile_pack(
                directory,
                records.iter().cloned().map(Ok),
                limits,
            )
            .map_err(|e| e.to_string())?;
            let mut manifest = current.clone();
            manifest.generation = next;
            manifest.deltas.push(descriptor);
            bace_storage_codec::write_manifest(directory, &manifest, limits)
                .map_err(|e| e.to_string())?;
            manifest
        }
        PackJob::Compact { current } => {
            if pressure.load(Ordering::Relaxed) {
                return Err("compaction deferred for save pressure".into());
            }
            let next = current
                .generation
                .checked_add(1)
                .ok_or("pack generation exhausted")?;
            let generation = current.open(directory, limits).map_err(|e| e.to_string())?;
            // Checked per output record; renewed save pressure aborts safely and
            // the returned original job can retry after persistence recovers.
            let base = generation
                .compact(directory, limits, pressure)
                .map_err(|e| e.to_string())?;
            let manifest = PackManifest {
                version: 1,
                generation: next,
                base,
                deltas: Vec::new(),
            };
            bace_storage_codec::write_manifest(directory, &manifest, limits)
                .map_err(|e| e.to_string())?;
            manifest
        }
    };
    let generation = Arc::new(
        manifest
            .open(directory, limits)
            .map_err(|e| e.to_string())?,
    );
    Ok(PreparedPack {
        manifest,
        generation,
    })
}
