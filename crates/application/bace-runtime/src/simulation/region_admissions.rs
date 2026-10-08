//! A rejected admission retains its whole prepared owner under output pressure.
use super::*;
use bace_simulation::RegionAdmissionOutcome;
#[derive(Default)]
pub struct RecoveredRegionAdmissions {
    pub outcomes: Vec<RegionAdmissionOutcome>,
}
impl RecoveredRegionAdmissions {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}
pub(super) struct RegionAdmissionReceivers {
    outcomes: mpsc::Receiver<RegionAdmissionOutcome>,
}
pub(super) struct RegionAdmissionSenders {
    outcomes: mpsc::SyncSender<RegionAdmissionOutcome>,
}
pub(super) fn channels(_capacity: usize) -> (RegionAdmissionSenders, RegionAdmissionReceivers) {
    let (outcomes, receiver) = mpsc::sync_channel(1);
    (
        RegionAdmissionSenders { outcomes },
        RegionAdmissionReceivers { outcomes: receiver },
    )
}
impl RegionAdmissionSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.take_region_admission_outcome() else {
                break;
            };
            if let Err(
                mpsc::TrySendError::Full(outcome) | mpsc::TrySendError::Disconnected(outcome),
            ) = self.outcomes.try_send(outcome)
            {
                assert!(
                    kernel.restore_region_admission_outcome(outcome).is_ok(),
                    "single-owner drained admission slot"
                );
                break;
            }
        }
    }
}
impl RegionAdmissionReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredRegionAdmissions) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}
impl SimulationWorker {
    pub fn region_admission_outcomes(&self) -> &mpsc::Receiver<RegionAdmissionOutcome> {
        &self.region_admissions.outcomes
    }
}

impl SimulationInput {
    pub fn try_admit_resident_region(
        &self,
        correlation: u64,
        prepared: bace_simulation::PreparedResidentRegion,
    ) -> Result<(), Box<bace_simulation::PreparedResidentRegion>> {
        let command = Command::AdmitResidentRegion(bace_simulation::RegionAdmissionRequest {
            correlation,
            prepared: Box::new(prepared),
        });
        match self.try_submit(command) {
            Ok(()) => Ok(()),
            Err(
                mpsc::TrySendError::Full(Command::AdmitResidentRegion(request))
                | mpsc::TrySendError::Disconnected(Command::AdmitResidentRegion(request)),
            ) => Err(request.prepared),
            Err(_) => unreachable!("exact submitted command returned"),
        }
    }
}
