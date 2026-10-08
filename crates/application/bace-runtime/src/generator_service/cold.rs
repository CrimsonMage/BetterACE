//! One bounded worker owns DAT reads, treasure/equipment preparation and binding.
use super::*;
use std::{sync::mpsc, thread};
pub(super) enum Job {
    Materialize {
        request: GeneratorHostRequest,
        region: Arc<PreparedRegionActivation>,
        budget: usize,
    },
    Bind {
        request: GeneratorHostRequest,
        region: Arc<PreparedRegionActivation>,
        raw: Arc<materialization::Materialized>,
    },
}
pub(super) enum Value {
    Materialized {
        raw: Arc<materialization::Materialized>,
        bytes: usize,
    },
    Bound {
        publications: Vec<PreparedGeneratorPublication>,
        ready: Box<materialization::Ready>,
        batch: crate::region_service::PreparedRegionSources,
    },
}
pub(super) struct Completion {
    pub key: GeneratorSpawnKey,
    pub value: Result<Value, String>,
}
pub(super) struct Worker {
    jobs: mpsc::SyncSender<Job>,
    results: mpsc::Receiver<Completion>,
    thread: thread::JoinHandle<()>,
}
impl Worker {
    pub fn start(config: &GeneratorServiceConfig) -> Result<Self, String> {
        let (jobs, inbox) = mpsc::sync_channel::<Job>(1);
        let (outbox, results) = mpsc::sync_channel(1);
        let manifest = config.assets.clone();
        let random = config.random.clone();
        let treasure = config.treasure_assets.clone();
        let drop_plain_wield = config.drop_plain_wield;
        let thread = thread::Builder::new()
            .name("bace-generator-preparation".into())
            .spawn(move || {
                let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest);
                let mut table = None;
                while let Ok(job) = inbox.recv() {
                    let (key, value) = match job {
                        Job::Materialize {
                            request,
                            region,
                            budget,
                        } => {
                            let result = (|| {
                                let raw = materialization::materialize(
                                    &region,
                                    &request,
                                    &treasure,
                                    &random,
                                    drop_plain_wield,
                                )?;
                                let bytes = raw.bytes()?;
                                if bytes > budget {
                                    return Err(
                                        "generator service materialization byte capacity".into()
                                    );
                                }
                                Ok(Value::Materialized {
                                    raw: Arc::new(raw),
                                    bytes,
                                })
                            })();
                            (request.intent.key, result)
                        }
                        Job::Bind {
                            request,
                            region,
                            raw,
                        } => {
                            let result = (|| {
                                let mut ready = materialization::bind(
                                    assets.as_mut().map_err(|e| e.clone()),
                                    &mut table,
                                    &region.generation,
                                    &region,
                                    &request,
                                    &raw,
                                )?;
                                let publications = publications::prepare(
                                    assets.as_mut().map_err(|e| e.clone()),
                                    &region,
                                    &request,
                                    &raw,
                                    &mut ready,
                                )?;
                                let batch = crate::region_service::PreparedRegionSources::prepare(
                                    std::mem::take(&mut ready.sources),
                                )?;
                                Ok(Value::Bound {
                                    publications,
                                    ready: Box::new(ready),
                                    batch,
                                })
                            })();
                            (request.intent.key, result)
                        }
                    };
                    if outbox.send(Completion { key, value }).is_err() {
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
    pub fn submit(&self, job: Job) -> bool {
        self.jobs.try_send(job).is_ok()
    }
    pub fn receive(&self) -> Result<Option<Completion>, String> {
        match self.results.try_recv() {
            Ok(v) => Ok(Some(v)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("generator preparation worker disconnected".into())
            }
        }
    }
    pub fn shutdown(self) -> Result<(), String> {
        drop(self.jobs);
        let remaining: Vec<_> = self.results.into_iter().collect();
        self.thread
            .join()
            .map_err(|_| "generator worker panicked")?;
        if !remaining.is_empty() {
            return Err("generator worker stopped with unclaimed completion".into());
        }
        Ok(())
    }
}
