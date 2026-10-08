//! One bounded cold owner prepares NPC and forced-target motion action chains.
use bace_types::EntityId;
use std::{
    sync::{Arc, mpsc},
    thread,
};
#[derive(Clone)]
pub struct NpcMotionWork {
    pub source: EntityId,
    pub actor: EntityId,
    pub ticket: u64,
    pub epoch: u16,
    pub revision: u64,
    pub table: u32,
    pub before: bace_motion::SourceMotionState,
    pub scale: f32,
    pub motion: u32,
    pub speed: f32,
    pub target: bool,
    pub style: Option<u32>,
    pub substyle: Option<u32>,
}
pub struct NpcMotionResult {
    pub work: Arc<NpcMotionWork>,
    pub result: Result<Arc<bace_motion::PreparedMotionChain>, String>,
}
pub struct NpcMotionWorker {
    pending: std::cell::Cell<usize>,
    jobs: mpsc::SyncSender<Arc<NpcMotionWork>>,
    results: mpsc::Receiver<NpcMotionResult>,
    thread: thread::JoinHandle<()>,
}
impl NpcMotionWorker {
    pub fn start(manifest: crate::region_activation::RegionAssetManifest) -> Result<Self, String> {
        let (jobs, inbox) = mpsc::sync_channel::<Arc<NpcMotionWork>>(1);
        let (outbox, results) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("bace-npc-motion".into())
            .spawn(move || {
                let mut assets = None;
                while let Ok(work) = inbox.recv() {
                    let result = (|| {
                        if assets.is_none() {
                            assets = Some(crate::region_activation::VerifiedRegionAssets::open(
                                &manifest,
                            )?);
                        }
                        assets
                            .as_mut()
                            .expect("verified NPC motions")
                            .prepare_npc_script_motion(&work)
                    })();
                    if outbox.send(NpcMotionResult { work, result }).is_err() {
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
    pub fn submit(&self, work: Arc<NpcMotionWork>) -> Result<(), Arc<NpcMotionWork>> {
        self.jobs
            .try_send(work)
            .map_err(|e| match e {
                mpsc::TrySendError::Full(work) | mpsc::TrySendError::Disconnected(work) => work,
            })
            .map(|()| self.pending.set(self.pending.get() + 1))
    }
    pub fn poll(&self) -> Result<Option<NpcMotionResult>, String> {
        match self.results.try_recv() {
            Ok(result) => {
                self.pending.set(self.pending.get().saturating_sub(1));
                Ok(Some(result))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("NPC motion worker disconnected".into()),
        }
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
    pub fn shutdown(self) -> Result<Vec<NpcMotionResult>, String> {
        drop(self.jobs);
        let results = self.results.into_iter().collect();
        self.thread
            .join()
            .map_err(|_| "NPC motion worker panicked")?;
        Ok(results)
    }
}
