//! One fixed blocking worker, one exact retained request and one result slot.
use super::*;
use std::{sync::mpsc, thread};
pub struct StaffNativeMagicWorker {
    sender: Option<mpsc::SyncSender<(Arc<StaffNativeMagicWork>, Arc<PackGeneration>)>>,
    generation: Arc<PackGeneration>,
    results: mpsc::Receiver<StaffNativeMagicResult>,
    task: Option<thread::JoinHandle<()>>,
    pending: Option<Arc<StaffNativeMagicWork>>,
}
impl StaffNativeMagicWorker {
    pub fn start(
        manifest: RegionAssetManifest,
        generation: Arc<PackGeneration>,
    ) -> Result<Self, String> {
        let (sender, requests) =
            mpsc::sync_channel::<(Arc<StaffNativeMagicWork>, Arc<PackGeneration>)>(1);
        let (output, results) = mpsc::sync_channel(1);
        let initial_generation = generation.clone();
        let task = thread::Builder::new()
            .name("bace-staff-native".into())
            .spawn(move || {
                let mut assets = None;
                while let Ok((work, generation)) = requests.recv() {
                    let result = (|| {
                        if assets.is_none() {
                            assets =
                                Some(StaffNativeMagicAssets::open(&manifest, generation.clone())?);
                        }
                        let assets = assets.as_mut().expect("native assets");
                        assets.generation = generation;
                        assets.prepare(&work)
                    })();
                    if output
                        .send(StaffNativeMagicResult { work, result })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sender: Some(sender),
            generation: initial_generation,
            results,
            task: Some(task),
            pending: None,
        })
    }
    pub fn submit(
        &mut self,
        work: Arc<StaffNativeMagicWork>,
    ) -> Result<(), Arc<StaffNativeMagicWork>> {
        self.submit_generation(work, self.generation.clone())
    }
    pub fn submit_generation(
        &mut self,
        work: Arc<StaffNativeMagicWork>,
        generation: Arc<PackGeneration>,
    ) -> Result<(), Arc<StaffNativeMagicWork>> {
        if work.token == 0 || self.pending.is_some() {
            return Err(work);
        }
        if self
            .sender
            .as_ref()
            .is_none_or(|sender| sender.try_send((work.clone(), generation.clone())).is_err())
        {
            return Err(work);
        }
        self.generation = generation;
        self.pending = Some(work);
        Ok(())
    }
    pub fn poll(&mut self) -> Result<Option<StaffNativeMagicResult>, String> {
        let result = match self.results.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("staff native worker stopped".into());
            }
        };
        if self
            .pending
            .as_ref()
            .is_none_or(|p| !Arc::ptr_eq(p, &result.work))
        {
            return Err("staff native worker correlation".into());
        }
        self.pending = None;
        Ok(Some(result))
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    /// Idle-only nonblocking handoff to the owning runtime shutdown controller.
    pub fn try_shutdown(&mut self) -> Result<Option<std::thread::JoinHandle<()>>, String> {
        if self.pending.is_some() {
            return Err("native magic preparation still pending".into());
        }
        self.sender.take();
        Ok(self.task.take())
    }
    pub fn shutdown(mut self) -> Result<(), Box<Self>> {
        if self.pending.is_some() {
            return Err(Box::new(self));
        }
        self.sender.take();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
        Ok(())
    }
}
