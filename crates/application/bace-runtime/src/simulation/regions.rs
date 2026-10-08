//! Region preparation requests remain on the simulation owner under backpressure.
use super::*;
use bace_simulation::RegionLifecycleEvent;

#[derive(Default)]
pub struct RecoveredRegions {
    pub events: Vec<RegionLifecycleEvent>,
    pub unloads: Vec<bace_simulation::RegionUnloadTicket>,
}
impl RecoveredRegions {
    pub(super) fn has_state(&self) -> bool {
        !self.events.is_empty() || !self.unloads.is_empty()
    }
}
pub(super) struct RegionReceivers {
    events: mpsc::Receiver<RegionLifecycleEvent>,
    unloads: mpsc::Receiver<bace_simulation::RegionUnloadTicket>,
}
pub(super) struct RegionSenders {
    pending: std::cell::RefCell<Option<bace_simulation::RegionUnloadTicket>>,
    events: mpsc::SyncSender<RegionLifecycleEvent>,
    unloads: mpsc::SyncSender<bace_simulation::RegionUnloadTicket>,
}
pub(super) fn channels(capacity: usize) -> (RegionSenders, RegionReceivers) {
    let (events, receiver) = mpsc::sync_channel(capacity);
    let (unloads, unload_receiver) = mpsc::sync_channel(1);
    (
        RegionSenders {
            events,
            unloads,
            pending: Default::default(),
        },
        RegionReceivers {
            events: receiver,
            unloads: unload_receiver,
        },
    )
}
impl RegionSenders {
    pub(super) fn recover_pending(&self) -> RecoveredRegions {
        RecoveredRegions {
            events: vec![],
            unloads: self.pending.borrow_mut().take().into_iter().collect(),
        }
    }
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        let pending = self
            .pending
            .borrow_mut()
            .take()
            .or_else(|| kernel.take_region_unload_proposal());
        if let Some(ticket) = pending
            && let Err(mpsc::TrySendError::Full(ticket) | mpsc::TrySendError::Disconnected(ticket)) =
                self.unloads.try_send(ticket)
        {
            // Retain the immutable capture instead of copying all item metadata each tick.
            *self.pending.borrow_mut() = Some(ticket);
        }
        for _ in 0..budget {
            let Some(event) = kernel.peek_region_lifecycle_event() else {
                break;
            };
            if self.events.try_send(event).is_err() {
                break;
            }
            kernel.take_region_lifecycle_event();
        }
    }
}
impl RegionReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredRegions) {
        output.events.extend(self.events.try_iter());
        output.unloads.extend(self.unloads.try_iter());
    }
}
impl SimulationWorker {
    pub fn region_unload_proposals(&self) -> &mpsc::Receiver<bace_simulation::RegionUnloadTicket> {
        &self.regions.unloads
    }
    pub fn region_lifecycle_events(&self) -> &mpsc::Receiver<RegionLifecycleEvent> {
        &self.regions.events
    }
}
