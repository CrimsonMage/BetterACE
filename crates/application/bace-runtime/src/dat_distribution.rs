//! Dedicated bounded DAT preparation. Construction takes already opened,
//! fingerprint-verified archives from asset admission. No socket/simulation work
//! executes file reads or compression; one blocking OS thread owns the archives.
use bace_dat::DatArchive;
use bace_dat_service::{
    DddError, DddJob, DddLimits, DddSession, PreparedRecord, prepare_archive_record,
};
use bace_wire::DddDatabase;
use std::{
    collections::BTreeMap,
    io,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatSubmissionReason {
    Full,
    Closed,
    SequenceExhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DatSubmissionError {
    pub job: DddJob,
    pub reason: DatSubmissionReason,
}

pub struct DatPreparationCompletion {
    pub job: DddJob,
    pub result: Result<PreparedRecord, DddError>,
}
impl DatPreparationCompletion {
    /// Only the matching generation's in-flight job can change session state.
    /// Failed preparation reopens the retained job for explicit retry. Encoded
    /// success must enter reliable output or remain retained by the caller.
    pub fn apply(self, session: &mut DddSession) -> Result<Vec<u8>, DddError> {
        match self.result {
            Ok(record) => session.complete_job(self.job, &record),
            Err(error) => {
                session.retry_job(self.job)?;
                Err(error)
            }
        }
    }
}

#[derive(Default)]
struct Counters {
    submitted: AtomicU64,
    prepared: AtomicU64,
    delivered: AtomicU64,
    abandoned: AtomicU64,
}
#[derive(Clone, Default)]
pub struct DatPreparationObserver {
    counters: Arc<Counters>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DatPreparationCounts {
    pub submitted: u64,
    /// Includes explicit preparation errors, not only successful records.
    pub prepared: u64,
    /// Results handed to a caller through receive or explicit shutdown.
    pub delivered: u64,
    /// Results/jobs abandoned by dropping the worker without explicit shutdown.
    pub abandoned: u64,
}
impl DatPreparationObserver {
    pub fn counts(&self) -> DatPreparationCounts {
        DatPreparationCounts {
            submitted: self.counters.submitted.load(Ordering::Relaxed),
            prepared: self.counters.prepared.load(Ordering::Relaxed),
            delivered: self.counters.delivered.load(Ordering::Relaxed),
            abandoned: self.counters.abandoned.load(Ordering::Relaxed),
        }
    }
}

struct Work {
    sequence: u64,
    job: DddJob,
}
struct Completed {
    sequence: u64,
    completion: DatPreparationCompletion,
}
#[must_use = "explicitly shut down to recover every accepted DAT preparation result"]
pub struct DatPreparationWorker {
    input: Option<SyncSender<Work>>,
    output: Receiver<Completed>,
    thread: Option<JoinHandle<()>>,
    outstanding: BTreeMap<u64, DddJob>,
    next_sequence: u64,
    capacity: usize,
    observer: DatPreparationObserver,
}
/// Finite drain result. Pending jobs after a thread panic remain owned here for
/// retry; there is no fabricated data or successful completion for those jobs.
pub struct DatPreparationDrain {
    pub completions: Vec<DatPreparationCompletion>,
    pub unrecovered_jobs: Vec<DddJob>,
    pub panicked: bool,
}
impl DatPreparationWorker {
    /// Archive opening/fingerprint admission happens before this call on asset
    /// capacity. At most `capacity` jobs may be outstanding across both channels
    /// and the active worker, capped at 64 MiB of prepared payload reservation.
    pub fn spawn(
        archives: Vec<(DddDatabase, DatArchive)>,
        limits: DddLimits,
        capacity: usize,
    ) -> io::Result<Self> {
        limits
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        if !(1..=1024).contains(&capacity)
            || archives.is_empty()
            || archives.len() > 4
            || limits.max_record_bytes == 0
            || capacity
                .checked_mul(limits.max_record_bytes)
                .is_none_or(|bytes| bytes > 64 * 1024 * 1024)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid DAT worker budget or archive set",
            ));
        }
        for (index, (kind, archive)) in archives.iter().enumerate() {
            if archive.header().dataset != kind.wire_identity().1
                || archives[..index]
                    .iter()
                    .any(|(previous, _)| previous == kind)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "DAT database identity mismatch or duplicate",
                ));
            }
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let (output, results) = mpsc::sync_channel(capacity);
        let observer = DatPreparationObserver::default();
        let thread_observer = observer.clone();
        let thread = thread::Builder::new()
            .name("bace-dat-preparation".into())
            .spawn(move || {
                drive(archives, limits, receiver, output, thread_observer);
            })?;
        Ok(Self {
            input: Some(sender),
            output: results,
            thread: Some(thread),
            outstanding: BTreeMap::new(),
            next_sequence: 0,
            capacity,
            observer,
        })
    }
    pub fn observer(&self) -> DatPreparationObserver {
        self.observer.clone()
    }
    pub fn pending_jobs(&self) -> usize {
        self.outstanding.len()
    }
    /// No I/O, compression or waiting. The exact rejected job remains owned by
    /// the caller; its DddSession in-flight state must be retried explicitly.
    pub fn try_submit(&mut self, job: DddJob) -> Result<(), DatSubmissionError> {
        let Some(sender) = &self.input else {
            return Err(DatSubmissionError {
                job,
                reason: DatSubmissionReason::Closed,
            });
        };
        if self.outstanding.len() >= self.capacity {
            return Err(DatSubmissionError {
                job,
                reason: DatSubmissionReason::Full,
            });
        }
        let Some(next) = self.next_sequence.checked_add(1) else {
            return Err(DatSubmissionError {
                job,
                reason: DatSubmissionReason::SequenceExhausted,
            });
        };
        match sender.try_send(Work {
            sequence: self.next_sequence,
            job,
        }) {
            Ok(()) => {
                self.outstanding.insert(self.next_sequence, job);
                self.next_sequence = next;
                self.observer
                    .counters
                    .submitted
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(TrySendError::Full(work)) => Err(DatSubmissionError {
                job: work.job,
                reason: DatSubmissionReason::Full,
            }),
            Err(TrySendError::Disconnected(work)) => Err(DatSubmissionError {
                job: work.job,
                reason: DatSubmissionReason::Closed,
            }),
        }
    }
    pub fn try_receive(&mut self) -> Result<Option<DatPreparationCompletion>, TryRecvError> {
        match self.output.try_recv() {
            Ok(completed) => {
                self.outstanding.remove(&completed.sequence);
                self.observer
                    .counters
                    .delivered
                    .fetch_add(1, Ordering::Relaxed);
                Ok(Some(completed.completion))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(error) => Err(error),
        }
    }
    /// Closes submission, drains all bounded accepted work/results, then joins.
    /// This blocks on asset I/O and belongs to lifecycle orchestration, never
    /// simulation or the network thread. No fixed latency is promised for a
    /// filesystem outage. Returned results must still be applied or cancelled.
    pub fn shutdown(mut self) -> DatPreparationDrain {
        self.drain(true)
    }
    fn drain(&mut self, delivered: bool) -> DatPreparationDrain {
        self.input.take();
        let mut completions = Vec::with_capacity(self.outstanding.len());
        for completed in &self.output {
            self.outstanding.remove(&completed.sequence);
            completions.push(completed.completion);
        }
        let panicked = self
            .thread
            .take()
            .is_some_and(|thread| thread.join().is_err());
        let unrecovered_jobs: Vec<_> = std::mem::take(&mut self.outstanding)
            .into_values()
            .collect();
        if delivered {
            self.observer
                .counters
                .delivered
                .fetch_add(completions.len() as u64, Ordering::Relaxed);
        } else {
            self.observer.counters.abandoned.fetch_add(
                (completions.len() + unrecovered_jobs.len()) as u64,
                Ordering::Relaxed,
            );
        }
        DatPreparationDrain {
            completions,
            unrecovered_jobs,
            panicked,
        }
    }
}
impl Drop for DatPreparationWorker {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.drain(false);
        }
    }
}
fn drive(
    mut archives: Vec<(DddDatabase, DatArchive)>,
    limits: DddLimits,
    input: Receiver<Work>,
    output: SyncSender<Completed>,
    observer: DatPreparationObserver,
) {
    for work in input {
        let result = archives
            .iter_mut()
            .find(|(kind, _)| *kind == work.job.record.database)
            .ok_or(DddError::MissingDatabase)
            .and_then(|(_, archive)| {
                let metadata = archive
                    .records()
                    .get(&work.job.record.object_id)
                    .ok_or(DddError::NotFound)?;
                if metadata.iteration != work.job.record.iteration
                    || metadata.size != work.job.record.raw_size
                {
                    return Err(DddError::PreparedRecordMismatch);
                }
                let prepared = prepare_archive_record(
                    archive,
                    work.job.record.database,
                    work.job.record.object_id,
                    work.job.record.resource_type,
                    limits,
                )?;
                if prepared.metadata() != work.job.record {
                    return Err(DddError::PreparedRecordMismatch);
                }
                Ok(prepared)
            });
        observer.counters.prepared.fetch_add(1, Ordering::Relaxed);
        // Backpressure retains this exact result. Explicit shutdown drains the
        // receiving side concurrently before joining, avoiding a full-queue join.
        if output
            .send(Completed {
                sequence: work.sequence,
                completion: DatPreparationCompletion {
                    job: work.job,
                    result,
                },
            })
            .is_err()
        {
            // Only possible if ownership was externally torn down; this worker's
            // public API retains its receiver until after join.
            observer.counters.abandoned.fetch_add(1, Ordering::Relaxed);
            break;
        }
    }
}
