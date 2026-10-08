//! Independent bounded outputs for staff operations and recall/binding workflows.
use super::*;
use bace_gameplay_api::experience::ExperienceEvent;
use bace_gameplay_api::staff::StaffEvent;
use bace_simulation::{ItemExperienceEvent, RecallEvent};
#[derive(Default)]
pub struct RecoveredInteractions {
    pub staff: Vec<StaffEvent>,
    pub recalls: Vec<RecallEvent>,
    pub portal_resolutions: Vec<bace_simulation::PortalResolutionOutcome>,
    pub experience: Vec<ExperienceEvent>,
    pub item_experience: Vec<ItemExperienceEvent>,
}
impl RecoveredInteractions {
    pub(super) fn has_state(&self) -> bool {
        !self.staff.is_empty()
            || !self.recalls.is_empty()
            || !self.portal_resolutions.is_empty()
            || !self.experience.is_empty()
            || !self.item_experience.is_empty()
    }
}
pub(super) struct InteractionReceivers {
    staff: mpsc::Receiver<StaffEvent>,
    recalls: mpsc::Receiver<RecallEvent>,
    portal_resolutions: mpsc::Receiver<bace_simulation::PortalResolutionOutcome>,
    experience: mpsc::Receiver<ExperienceEvent>,
    item_experience: mpsc::Receiver<ItemExperienceEvent>,
}
pub(super) struct InteractionSenders {
    staff: mpsc::SyncSender<StaffEvent>,
    recalls: mpsc::SyncSender<RecallEvent>,
    portal_resolutions: mpsc::SyncSender<bace_simulation::PortalResolutionOutcome>,
    experience: mpsc::SyncSender<ExperienceEvent>,
    item_experience: mpsc::SyncSender<ItemExperienceEvent>,
}
pub(super) fn channels(capacity: usize) -> (InteractionSenders, InteractionReceivers) {
    let (s, sr) = mpsc::sync_channel(capacity);
    let (r, rr) = mpsc::sync_channel(capacity);
    let (p, pr) = mpsc::sync_channel(capacity);
    let (e, er) = mpsc::sync_channel(capacity);
    let (i, ir) = mpsc::sync_channel(capacity);
    (
        InteractionSenders {
            staff: s,
            recalls: r,
            portal_resolutions: p,
            experience: e,
            item_experience: i,
        },
        InteractionReceivers {
            staff: sr,
            recalls: rr,
            portal_resolutions: pr,
            experience: er,
            item_experience: ir,
        },
    )
}
impl InteractionSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_portal_resolution() else {
                break;
            };
            if self.portal_resolutions.try_send(outcome.clone()).is_err() {
                break;
            }
            kernel.take_portal_resolution();
        }

        for _ in 0..budget {
            let Some(event) = kernel.peek_item_experience_event() else {
                break;
            };
            if self.item_experience.try_send(event.clone()).is_err() {
                break;
            }
            kernel.take_item_experience_event();
        }
        for _ in 0..budget {
            let Some(event) = kernel.peek_experience_event() else {
                break;
            };
            if self.experience.try_send(event.clone()).is_err() {
                break;
            }
            kernel.take_experience_event();
        }
        for _ in 0..budget {
            let Some(event) = kernel.peek_staff_event() else {
                break;
            };
            if self.staff.try_send(event.clone()).is_err() {
                break;
            }
            kernel.take_staff_event();
        }
        for _ in 0..budget {
            let Some(event) = kernel.peek_recall_event() else {
                break;
            };
            if self.recalls.try_send(event.clone()).is_err() {
                break;
            }
            kernel.take_recall_event();
        }
    }
}
impl InteractionReceivers {
    pub(super) fn recover(&self, out: &mut RecoveredInteractions) {
        out.staff.extend(self.staff.try_iter());
        out.recalls.extend(self.recalls.try_iter());
        out.portal_resolutions
            .extend(self.portal_resolutions.try_iter());
        out.experience.extend(self.experience.try_iter());
        out.item_experience.extend(self.item_experience.try_iter());
    }
}
impl SimulationWorker {
    pub fn portal_resolutions(&self) -> &mpsc::Receiver<bace_simulation::PortalResolutionOutcome> {
        &self.interactions.portal_resolutions
    }
    pub fn item_experience_events(&self) -> &mpsc::Receiver<ItemExperienceEvent> {
        &self.interactions.item_experience
    }
    pub fn experience_events(&self) -> &mpsc::Receiver<ExperienceEvent> {
        &self.interactions.experience
    }
    pub fn staff_events(&self) -> &mpsc::Receiver<StaffEvent> {
        &self.interactions.staff
    }
    pub fn recall_events(&self) -> &mpsc::Receiver<RecallEvent> {
        &self.interactions.recalls
    }
}
