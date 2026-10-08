//! One cold worker; admission counts executing and unclaimed results together.
use std::{collections::BTreeSet, sync::mpsc, thread};
pub(super) struct Lane<J, V> {
    jobs: mpsc::SyncSender<(u64, J)>,
    results: mpsc::Receiver<(u64, V)>,
    thread: thread::JoinHandle<()>,
    outstanding: BTreeSet<u64>,
    capacity: usize,
}
impl<J: Send + 'static, V: Send + 'static> Lane<J, V> {
    pub fn start(
        capacity: usize,
        mut prepare: impl FnMut(J) -> V + Send + 'static,
    ) -> Result<Self, String> {
        if !(1..=4).contains(&capacity) {
            return Err("player preparation capacity must be 1..4".into());
        }
        let (jobs, inbox) = mpsc::sync_channel::<(u64, J)>(capacity);
        let (outbox, results) = mpsc::sync_channel(capacity);
        let thread = thread::Builder::new()
            .name("bace-player-preparation".into())
            .spawn(move || {
                while let Ok((correlation, job)) = inbox.recv() {
                    let value = prepare(job);
                    if outbox.send((correlation, value)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            jobs,
            results,
            thread,
            outstanding: BTreeSet::new(),
            capacity,
        })
    }
    pub fn try_submit(&mut self, correlation: u64, job: J) -> Result<(), J> {
        if correlation == 0
            || self.outstanding.len() == self.capacity
            || self.outstanding.contains(&correlation)
        {
            return Err(job);
        }
        match self.jobs.try_send((correlation, job)) {
            Ok(()) => {
                self.outstanding.insert(correlation);
                Ok(())
            }
            Err(
                mpsc::TrySendError::Full((_, job)) | mpsc::TrySendError::Disconnected((_, job)),
            ) => Err(job),
        }
    }
    pub fn try_recv(&mut self) -> Result<Option<V>, String> {
        match self.results.try_recv() {
            Ok((correlation, value)) => {
                if !self.outstanding.remove(&correlation) {
                    return Err("player preparation completion fence mismatch".into());
                }
                Ok(Some(value))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("player preparation worker disconnected; pending requests retained".into())
            }
        }
    }
    pub fn pending(&self) -> usize {
        self.outstanding.len()
    }
    pub fn try_shutdown(self) -> Result<thread::JoinHandle<()>, Box<Self>> {
        if !self.outstanding.is_empty() {
            return Err(Box::new(self));
        }
        drop(self.jobs);
        drop(self.results);
        Ok(self.thread)
    }
}
#[cfg(test)]
mod tests;
