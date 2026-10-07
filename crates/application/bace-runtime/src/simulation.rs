//! One operating-system thread owns the kernel and all authoritative mutation.
//! Network/database adapters may only submit bounded commands; they never borrow
//! the world. This runtime boundary currently drives the synthetic kernel.
use bace_gameplay_api::ProgressionOutcome;
use bace_simulation::{Command, Kernel, SimulationError};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const STEP: Duration = Duration::from_nanos(1_000_000_000 / 30);
const MAX_CAPACITY: usize = 65_536;

pub struct SimulationConfig {
    pub command_capacity: usize,
    /// None runs until explicit shutdown. A limit is useful for replay/exercises.
    pub tick_limit: Option<u64>,
    /// Unpaced execution is for synthetic measurement/replay only.
    pub real_time: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            command_capacity: 8192,
            tick_limit: None,
            real_time: true,
        }
    }
}

/// A bounded sender returns ownership of commands on overload/disconnection.
#[derive(Clone)]
pub struct SimulationInput {
    sender: mpsc::SyncSender<Command>,
    admission: Arc<Mutex<bool>>,
}

impl SimulationInput {
    /// Success acknowledges enqueueing only, not validation or application.
    /// Shutdown may discard queued commands; the final report counts them.
    /// Never waits for channel capacity; a short admission gate serializes
    /// senders with closure, and is never acquired during a kernel tick.
    pub fn try_submit(&self, command: Command) -> Result<(), mpsc::TrySendError<Command>> {
        let open = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if !*open {
            return Err(mpsc::TrySendError::Disconnected(command));
        }
        self.sender.try_send(command)
    }
}

/// Closes admission on every exit, including errors and unwinding.
struct SimulationInbox {
    receiver: mpsc::Receiver<Command>,
    admission: Arc<Mutex<bool>>,
}

impl SimulationInbox {
    fn close_and_recover(&self) -> Vec<Command> {
        *self.admission.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.receiver.try_iter().collect()
    }
    fn close_and_discard(&self) -> u64 {
        self.close_and_recover().len() as u64
    }
}

impl Drop for SimulationInbox {
    fn drop(&mut self) {
        self.close_and_discard();
    }
}

#[must_use = "use shutdown_recover to retain character state and unapplied work"]
pub struct SimulationWorker {
    input: SimulationInput,
    stop: Arc<AtomicBool>,
    finite: bool,
    thread: Option<JoinHandle<SimulationExit>>,
    outcomes: mpsc::Receiver<ProgressionOutcome>,
}

/// Recovery transfers the single kernel owner; it never acknowledges persistence.
#[must_use = "retain the kernel and unresolved commands until lifecycle/save drain completes"]
pub struct SimulationExit {
    pub report: SimulationReport,
    pub kernel: Kernel,
    pub unprocessed_commands: Vec<Command>,
    pub undelivered_outcomes: Vec<ProgressionOutcome>,
    pub failure: Option<SimulationError>,
}
impl SimulationExit {
    fn requires_recovery(&self) -> bool {
        self.kernel.has_characters()
            || self.kernel.has_queued_progression()
            || !self.undelivered_outcomes.is_empty()
            || self.kernel.pending_progression_outcomes() != 0
            || self
                .unprocessed_commands
                .iter()
                .any(|command| matches!(command, Command::RaiseProgression { .. }))
    }
}
impl std::fmt::Debug for SimulationExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimulationExit")
            .field("report", &self.report)
            .field("has_characters", &self.kernel.has_characters())
            .field("unprocessed_commands", &self.unprocessed_commands.len())
            .field("undelivered_outcomes", &self.undelivered_outcomes.len())
            .field("failure", &self.failure)
            .finish()
    }
}

#[derive(Debug)]
pub struct SimulationReport {
    pub thread_id: thread::ThreadId,
    pub ticks: u64,
    pub rejected_commands: u64,
    /// Enqueued adapter commands left unprocessed at normal worker exit.
    /// Distinct from commands processed but rejected by the kernel.
    pub discarded_commands: u64,
    pub missed_deadlines: u64,
    pub max_tick: Duration,
    /// Upper bound of the 100-microsecond p99 histogram bucket; None means
    /// no ticks ran or p99 exceeded the final 25.5 ms bucket.
    pub p99_upper_bound: Option<Duration>,
}

/// Startup failure returns the only kernel owner, including unsaved revisions.
pub struct SimulationStartupFailure {
    pub kernel: Kernel,
    pub reason: String,
}
impl std::fmt::Debug for SimulationStartupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimulationStartupFailure")
            .field("reason", &self.reason)
            .field("has_characters", &self.kernel.has_characters())
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("command capacity must be 1..=65536 and tick limit must be nonzero")]
    Configuration,
    #[error("simulation startup failed; kernel ownership is retained: {0:?}")]
    StartupRecovery(Box<SimulationStartupFailure>),
    #[error("simulation state requires explicit owner recovery; no save drain was acknowledged")]
    RecoveryRequired(Box<SimulationExit>),
    #[error("wait requires a finite tick limit; the unbounded worker was stopped and joined")]
    UnboundedWait,
    #[error("simulation thread could not start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("simulation thread panicked")]
    Panic,
    #[error(transparent)]
    Simulation(#[from] SimulationError),
}

impl SimulationWorker {
    pub fn spawn(kernel: Kernel, config: SimulationConfig) -> Result<Self, WorkerError> {
        let output_capacity = config.command_capacity;
        Self::spawn_with_output_capacity(kernel, config, output_capacity)
    }
    pub fn spawn_with_output_capacity(
        kernel: Kernel,
        config: SimulationConfig,
        output_capacity: usize,
    ) -> Result<Self, WorkerError> {
        if !(1..=MAX_CAPACITY).contains(&config.command_capacity)
            || config.tick_limit == Some(0)
            || !(1..=MAX_CAPACITY).contains(&output_capacity)
        {
            return Err(WorkerError::StartupRecovery(Box::new(
                SimulationStartupFailure {
                    kernel,
                    reason: "invalid command/output capacity or tick limit".into(),
                },
            )));
        }
        let (sender, receiver) = mpsc::sync_channel(config.command_capacity);
        let admission = Arc::new(Mutex::new(true));
        let inbox = SimulationInbox {
            receiver,
            admission: Arc::clone(&admission),
        };
        let (outcome_sender, outcomes) = mpsc::sync_channel(output_capacity);
        let finite = config.tick_limit.is_some();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        // One startup-only handoff keeps state recoverable if OS thread creation
        // fails. Neither mutex nor shared kernel storage survives into ticks.
        let handoff = Arc::new(Mutex::new(Some(kernel)));
        let receive_kernel = handoff.clone();
        let thread = match thread::Builder::new()
            .name("bace-simulation".into())
            .spawn(move || {
                let kernel = receive_kernel
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("single kernel startup handoff");
                drop(receive_kernel);
                drive_owned(kernel, inbox, worker_stop, config, outcome_sender)
            }) {
            Ok(thread) => thread,
            Err(error) => {
                let kernel = handoff
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("failed spawn retains kernel");
                return Err(WorkerError::StartupRecovery(Box::new(
                    SimulationStartupFailure {
                        kernel,
                        reason: error.to_string(),
                    },
                )));
            }
        };
        Ok(Self {
            input: SimulationInput { sender, admission },
            stop,
            finite,
            thread: Some(thread),
            outcomes,
        })
    }

    pub fn input(&self) -> SimulationInput {
        self.input.clone()
    }

    pub fn outcomes(&self) -> &mpsc::Receiver<ProgressionOutcome> {
        &self.outcomes
    }

    /// Stops the single owner and returns all authoritative state, unapplied
    /// adapter requests and undelivered results. Caller still owns save drain.
    pub fn shutdown_recover(mut self) -> Result<SimulationExit, WorkerError> {
        self.request_stop();
        self.join_owned()
    }
    pub fn wait_recover(mut self) -> Result<SimulationExit, WorkerError> {
        if !self.finite {
            self.request_stop();
        }
        self.join_owned()
    }

    /// Close admission, finish any current tick, count queued discards and join.
    /// Persistence has its own drain protocol; this does not acknowledge saves.
    pub fn shutdown(mut self) -> Result<SimulationReport, WorkerError> {
        self.request_stop();
        self.join()
    }

    /// Wait for a configured finite exercise/replay to finish.
    /// An unbounded worker is stopped and joined before returning an error.
    pub fn wait(mut self) -> Result<SimulationReport, WorkerError> {
        if !self.finite {
            self.request_stop();
            let exit = self.join_owned()?;
            if exit.requires_recovery() {
                return Err(WorkerError::RecoveryRequired(Box::new(exit)));
            }
            return Err(WorkerError::UnboundedWait);
        }
        self.join()
    }

    fn request_stop(&self) {
        *self
            .input
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = false;
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }

    fn join_owned(&mut self) -> Result<SimulationExit, WorkerError> {
        let mut exit = self
            .thread
            .take()
            .ok_or(WorkerError::Panic)?
            .join()
            .map_err(|_| WorkerError::Panic)?;
        // Channel results precede the retained overflow result and kernel outbox.
        let mut delivered: Vec<_> = self.outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_outcomes);
        exit.undelivered_outcomes = delivered;
        Ok(exit)
    }
    fn join(&mut self) -> Result<SimulationReport, WorkerError> {
        let exit = self.join_owned()?;
        if exit.requires_recovery() {
            return Err(WorkerError::RecoveryRequired(Box::new(exit)));
        }
        if let Some(error) = exit.failure {
            return Err(WorkerError::Simulation(error));
        }
        Ok(exit.report)
    }
}

impl Drop for SimulationWorker {
    fn drop(&mut self) {
        self.request_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn drive_owned(
    mut kernel: Kernel,
    inbox: SimulationInbox,
    stop: Arc<AtomicBool>,
    config: SimulationConfig,
    output: mpsc::SyncSender<ProgressionOutcome>,
) -> SimulationExit {
    let mut report = SimulationReport {
        thread_id: thread::current().id(),
        ticks: 0,
        rejected_commands: 0,
        discarded_commands: 0,
        missed_deadlines: 0,
        max_tick: Duration::ZERO,
        p99_upper_bound: None,
    };
    let mut pending_command = None;
    let mut pending_outcome = None;
    let mut failure = None;
    let mut histogram = [0_u64; 256];
    let mut deadline = Instant::now();
    while !stop.load(Ordering::Acquire)
        && config.tick_limit.is_none_or(|limit| report.ticks < limit)
    {
        if config.real_time {
            // park may wake spuriously; the deadline remains authoritative.
            while Instant::now() < deadline && !stop.load(Ordering::Acquire) {
                thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
            }
            if stop.load(Ordering::Acquire) {
                break;
            }
        }
        let start = Instant::now();
        // Preserve output before draining new inputs. A stalled consumer applies
        // backpressure to commands while physics continues at its fixed step.
        flush_outcomes(
            &mut kernel,
            &output,
            &mut pending_outcome,
            config.command_capacity,
        );
        for _ in 0..config.command_capacity {
            let command = match pending_command
                .take()
                .or_else(|| inbox.receiver.try_recv().ok())
            {
                Some(command) => command,
                None => break,
            };
            if let Err(command) = kernel.try_enqueue(command) {
                pending_command = Some(command);
                break;
            }
        }
        match kernel.step() {
            Ok(rejected) => report.rejected_commands += rejected.len() as u64,
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
        flush_outcomes(
            &mut kernel,
            &output,
            &mut pending_outcome,
            config.command_capacity,
        );
        report.ticks += 1;
        let elapsed = start.elapsed();
        report.max_tick = report.max_tick.max(elapsed);
        let bucket = (elapsed.as_nanos().div_ceil(100_000).saturating_sub(1)).min(255) as usize;
        histogram[bucket] += 1;
        deadline += STEP;
        if config.real_time {
            let now = Instant::now();
            if now > deadline {
                let missed = (now.duration_since(deadline).as_nanos() / STEP.as_nanos() + 1) as u64;
                report.missed_deadlines += missed;
                // No busy catch-up spiral and no enlarged physics delta.
                deadline = now + STEP;
            }
        }
    }
    let mut unprocessed_commands = Vec::new();
    if let Some(command) = pending_command {
        unprocessed_commands.push(command);
    }
    unprocessed_commands.extend(inbox.close_and_recover());
    report.discarded_commands = unprocessed_commands.len() as u64;
    let undelivered_outcomes = pending_outcome.into_iter().collect();
    if report.ticks != 0 {
        let target = report.ticks.saturating_mul(99).div_ceil(100);
        let mut cumulative = 0;
        for (bucket, count) in histogram.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                report.p99_upper_bound =
                    (bucket < 255).then(|| Duration::from_micros((bucket as u64 + 1) * 100));
                break;
            }
        }
    }
    SimulationExit {
        report,
        kernel,
        unprocessed_commands,
        undelivered_outcomes,
        failure,
    }
}

fn flush_outcomes(
    kernel: &mut Kernel,
    output: &mpsc::SyncSender<ProgressionOutcome>,
    pending: &mut Option<ProgressionOutcome>,
    limit: usize,
) {
    for _ in 0..limit {
        let Some(outcome) = pending.take().or_else(|| kernel.take_progression_outcome()) else {
            break;
        };
        if let Err(error) = output.try_send(outcome) {
            *pending = Some(match error {
                mpsc::TrySendError::Full(value) | mpsc::TrySendError::Disconnected(value) => value,
            });
            break;
        }
    }
}

#[cfg(test)]
fn drive(
    kernel: Kernel,
    inbox: SimulationInbox,
    stop: Arc<AtomicBool>,
    config: SimulationConfig,
) -> Result<SimulationReport, SimulationError> {
    let (output, _receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let exit = drive_owned(kernel, inbox, stop, config, output);
    match exit.failure {
        Some(error) => Err(error),
        None => Ok(exit.report),
    }
}

#[cfg(test)]
mod tests;
