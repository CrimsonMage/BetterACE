//! One bounded cold DAT owner for authorized map destinations.
use crate::{region_activation::RegionAssetManifest, staff_map::StaffMapAssets};
use bace_gameplay_api::staff::{MapTeleportRequest, PreparedMapTeleport};
use std::{sync::mpsc, thread};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaffMapWork {
    pub token: u64,
    pub request: MapTeleportRequest,
    pub expected_epoch: u16,
}
pub struct StaffMapResult {
    pub work: StaffMapWork,
    pub result: Result<PreparedMapTeleport, String>,
}
pub struct StaffMapWorker {
    sender: Option<mpsc::SyncSender<StaffMapWork>>,
    results: mpsc::Receiver<StaffMapResult>,
    task: Option<thread::JoinHandle<()>>,
    pending: Option<StaffMapWork>,
}
impl StaffMapWorker {
    pub fn start(manifest: RegionAssetManifest) -> Result<Self, String> {
        let (sender, requests) = mpsc::sync_channel::<StaffMapWork>(1);
        let (output, results) = mpsc::sync_channel(1);
        let task = thread::Builder::new()
            .name("bace-staff-map".into())
            .spawn(move || {
                let mut assets = None;
                while let Ok(work) = requests.recv() {
                    let result = (|| {
                        if assets.is_none() {
                            assets = Some(StaffMapAssets::open(&manifest)?);
                        }
                        assets
                            .as_mut()
                            .expect("loaded map assets")
                            .prepare(work.request, work.expected_epoch)
                    })();
                    if output.send(StaffMapResult { work, result }).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sender: Some(sender),
            results,
            task: Some(task),
            pending: None,
        })
    }
    pub fn submit(&mut self, work: StaffMapWork) -> Result<(), StaffMapWork> {
        if work.token == 0 || self.pending.is_some() {
            return Err(work);
        }
        if self
            .sender
            .as_ref()
            .is_none_or(|s| s.try_send(work).is_err())
        {
            return Err(work);
        }
        self.pending = Some(work);
        Ok(())
    }
    pub fn poll(&mut self) -> Result<Option<StaffMapResult>, String> {
        let result = match self.results.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => return Err("staff map worker stopped".into()),
        };
        if self.pending != Some(result.work) {
            return Err("staff map worker correlation".into());
        }
        self.pending = None;
        Ok(Some(result))
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    /// Joining is a final lifecycle operation, never part of the simulation tick.
    /// In-flight work returns the complete worker instead of discarding a request.
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

#[cfg(test)]
mod tests;
